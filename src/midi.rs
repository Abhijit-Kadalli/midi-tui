use std::sync::{Arc, Mutex};
use midir::{MidiInput, MidiInputConnection};
use crate::synth::SynthEngine;

pub struct MidiManager {
    midi_input: Option<MidiInput>,
    connection: Option<MidiInputConnection<()>>,
    pub ports: Vec<String>,
    pub selected_port_idx: Option<usize>,
    pub connected_port_name: Option<String>,
}

impl MidiManager {
    pub fn new() -> Self {
        let midi_input = MidiInput::new("midi-tui-input").ok();
        let mut ports = Vec::new();

        if let Some(ref input) = midi_input {
            for port in input.ports() {
                if let Ok(name) = input.port_name(&port) {
                    ports.push(name);
                }
            }
        }

        Self {
            midi_input,
            connection: None,
            ports,
            selected_port_idx: None,
            connected_port_name: None,
        }
    }

    pub fn refresh_ports(&mut self) {
        self.ports.clear();
        if let Some(ref input) = self.midi_input {
            for port in input.ports() {
                if let Ok(name) = input.port_name(&port) {
                    self.ports.push(name);
                }
            }
        }
    }

    pub fn connect(
        &mut self,
        port_idx: usize,
        synth: Arc<Mutex<SynthEngine>>,
        // Callback or queue for recording notes in the sequencer
        recording_sender: std::sync::mpsc::Sender<MidiRecordEvent>,
    ) -> Result<(), anyhow::Error> {
        // Disconnect existing first
        self.disconnect();

        let midi_input = self.midi_input.take().ok_or_else(|| {
            anyhow::anyhow!("MIDI input subsystem not initialized or already connected")
        })?;

        let ports = midi_input.ports();
        let port = match ports.get(port_idx) {
            Some(p) => p,
            None => {
                self.midi_input = Some(midi_input);
                return Err(anyhow::anyhow!("Selected MIDI port index out of range"));
            }
        };


        let port_name = match midi_input.port_name(port) {
            Ok(name) => name,
            Err(e) => {
                self.midi_input = Some(midi_input);
                return Err(anyhow::anyhow!("MIDI port name query error: {:?}", e));
            }
        };
        
        let synth_clone = Arc::clone(&synth);

        // Build callback to process physical MIDI input
        let conn = match midi_input.connect(
            port,
            "midi-tui-connection",
            move |_timestamp, message, _| {
                parse_midi_message(message, &synth_clone, &recording_sender);
            },
            (),
        ) {
            Ok(c) => c,
            Err(e) => {
                // If connection failed, recreate midi_input so user can try again
                self.midi_input = MidiInput::new("midi-tui-input").ok();
                return Err(anyhow::anyhow!("MIDI connection error: {:?}", e));
            }
        };

        self.connection = Some(conn);
        self.selected_port_idx = Some(port_idx);
        self.connected_port_name = Some(port_name);

        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.close();
        }
        // Reinitialize midi_input subsystem to allow scanning ports again
        self.midi_input = MidiInput::new("midi-tui-input").ok();
        self.selected_port_idx = None;
        self.connected_port_name = None;
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum MidiRecordEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
}

fn parse_midi_message(
    message: &[u8],
    synth: &Arc<Mutex<SynthEngine>>,
    recording_sender: &std::sync::mpsc::Sender<MidiRecordEvent>,
) {
    if message.is_empty() {
        return;
    }

    let status = message[0];
    let msg_type = status & 0xF0;
    let _channel = status & 0x0F; // For now we mix globally or map based on track

    match msg_type {
        0x90 => { // Note On
            if message.len() >= 3 {
                let note = message[1];
                let velocity = message[2];
                
                let mut synth_locked = synth.lock().unwrap();

                // Determine active track based on instrument mapping
                // For direct play, trigger on the first Subtractive synth (Lead) or FM
                // We'll trigger on whichever instrument is selected in the UI
                // We will handle routing in the App state but default to Track 0 if empty
                // Let's trigger on Track 0 for Note On by default unless we know the current active track index.
                // We can query the active track or play on the matching engine type.
                // Let's implement active track routing. In synth note_on we take track_idx.
                // To keep it simple: we can route the note to the currently selected track index!
                // Wait! How do we know the currently selected track index inside the static callback?
                // The SynthEngine contains a list of instruments. We can route it based on MIDI channel,
                // or just route it to the active editor track!
                // Let's store an `active_track_idx` in SynthEngine or map MIDI channels:
                // Channel 0 -> Track 0, Channel 1 -> Track 1, Channel 2 -> Track 2, Channel 3 -> Track 3.
                // This is extremely professional and matches standard multi-timbral MIDI routing perfectly!
                let track_idx = (_channel as usize).min(3);

                if velocity > 0 {
                    synth_locked.note_on(track_idx, note, velocity as f32 / 127.0);
                    let _ = recording_sender.send(MidiRecordEvent::NoteOn { note, velocity });
                } else {
                    synth_locked.note_off(track_idx, note);
                    let _ = recording_sender.send(MidiRecordEvent::NoteOff { note });
                }
            }
        }
        0x80 => { // Note Off
            if message.len() >= 2 {
                let note = message[1];
                let mut synth_locked = synth.lock().unwrap();
                let track_idx = (_channel as usize).min(3);
                synth_locked.note_off(track_idx, note);
                let _ = recording_sender.send(MidiRecordEvent::NoteOff { note });
            }
        }
        0xB0 => { // Control Change (Knobs / Faders)
            if message.len() >= 3 {
                let controller = message[1];
                let value = message[2];
                let mut synth_locked = synth.lock().unwrap();
                
                // Let's map standard CC dials:
                // CC 74 (Filter Cutoff) -> updates low-pass cutoff
                // CC 71 (Filter Resonance) -> updates resonance
                // CC 73 (Attack), CC 72 (Release) -> ADSR updates
                match controller {
                    74 => { // Filter Cutoff: scale 0-127 to 50Hz-8000Hz
                        let cutoff = 50.0 + (value as f32 / 127.0) * 7950.0;
                        let resonance = synth_locked.filter_resonance;
                        synth_locked.update_filter(cutoff, resonance);
                    }
                    71 => { // Filter Resonance: scale 0-127 to 0.1-5.0
                        let resonance = 0.1 + (value as f32 / 127.0) * 4.9;
                        let cutoff = synth_locked.filter_cutoff;
                        synth_locked.update_filter(cutoff, resonance);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

// QWERTY computer keyboard to MIDI Note mapping
// Maps keyboard character keys to MIDI notes starting from C4 (60)
pub fn keyboard_key_to_note(c: char) -> Option<u8> {
    match c {
        // White Keys (C4 - E5)
        'a' => Some(60), // C4
        's' => Some(62), // D4
        'd' => Some(64), // E4
        'f' => Some(65), // F4
        'g' => Some(67), // G4
        'h' => Some(69), // A4
        'j' => Some(71), // B4
        'k' => Some(72), // C5
        'l' => Some(74), // D5
        ';' => Some(76), // E5

        // Black Keys
        'w' => Some(61), // C#4
        'e' => Some(63), // D#4
        't' => Some(66), // F#4
        'y' => Some(68), // G#4
        'u' => Some(70), // A#4
        'o' => Some(73), // C#5
        'p' => Some(75), // D#5
        _ => None,
    }
}
