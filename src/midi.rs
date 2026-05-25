use std::sync::{Arc, Mutex};
use midir::{MidiInput, MidiInputConnection};
use crate::synth::SynthEngine;

pub struct MidiManager {
    midi_input: Option<MidiInput>,
    connection: Option<MidiInputConnection<()>>,
    pub ports: Vec<String>,
    pub selected_port_idx: Option<usize>,
    pub connected_port_name: Option<String>,
    pub last_midi_message: Arc<Mutex<Option<String>>>,
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
            last_midi_message: Arc::new(Mutex::new(None)),
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
        let last_msg_clone = Arc::clone(&self.last_midi_message);

        // Build callback to process physical MIDI input
        let conn = match midi_input.connect(
            port,
            "midi-tui-connection",
            move |_timestamp, message, _| {
                parse_midi_message(message, &synth_clone, &recording_sender, &last_msg_clone);
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
        if let Ok(mut msg) = self.last_midi_message.lock() {
            *msg = Some("Waiting for MIDI input...".to_string());
        }

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
        if let Ok(mut msg) = self.last_midi_message.lock() {
            *msg = None;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum MidiRecordEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    ControlChange { controller: u8, value: u8 },
}

pub fn pitch_to_name(pitch: u8) -> String {
    let oct = (pitch / 12) as i32 - 1;
    let note_idx = pitch % 12;
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", names[note_idx as usize], oct)
}

fn get_cc_name(controller: u8) -> &'static str {
    match controller {
        1 => "Modulation Wheel",
        7 => "Volume Fader",
        10 => "Stereo Pan",
        11 => "Expression",
        64 => "Sustain Pedal",
        71 => "Filter Resonance (Q)",
        72 => "Release Time",
        73 => "Attack Time",
        74 => "Filter Cutoff (Freq)",
        75 => "Sustain Level",
        79 => "Decay Time",
        _ => "General Purpose Controller",
    }
}

fn parse_midi_message(
    message: &[u8],
    synth: &Arc<Mutex<SynthEngine>>,
    recording_sender: &std::sync::mpsc::Sender<MidiRecordEvent>,
    last_midi_message: &Arc<Mutex<Option<String>>>,
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
                // Channel 0 -> Track 0, Channel 1 -> Track 1, Channel 2 -> Track 2, Channel 3 -> Track 3.
                let track_idx = (_channel as usize).min(3);

                if velocity > 0 {
                    synth_locked.note_on(track_idx, note, velocity as f32 / 127.0);
                    let _ = recording_sender.send(MidiRecordEvent::NoteOn { note, velocity });
                    
                    let note_name = pitch_to_name(note);
                    let log_str = format!("Note ON: {} (MIDI Note {}, Velocity {})", note_name, note, velocity);
                    if let Ok(mut msg) = last_midi_message.lock() {
                        *msg = Some(log_str);
                    }
                } else {
                    synth_locked.note_off(track_idx, note);
                    let _ = recording_sender.send(MidiRecordEvent::NoteOff { note });
                    
                    let note_name = pitch_to_name(note);
                    let log_str = format!("Note OFF: {} (MIDI Note {})", note_name, note);
                    if let Ok(mut msg) = last_midi_message.lock() {
                        *msg = Some(log_str);
                    }
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
                
                let note_name = pitch_to_name(note);
                let log_str = format!("Note OFF: {} (MIDI Note {})", note_name, note);
                if let Ok(mut msg) = last_midi_message.lock() {
                    *msg = Some(log_str);
                }
            }
        }
        0xB0 => { // Control Change (Knobs / Faders)
            if message.len() >= 3 {
                let controller = message[1];
                let value = message[2];
                let mut synth_locked = synth.lock().unwrap();
                let track_idx = (_channel as usize).min(3);
                
                // Map standard CC dials to control volumes, panning, ADSR envelope settings, and filters
                match controller {
                    7 => { // Volume fader: scale 0-127 to 0.0-1.0
                        synth_locked.instruments[track_idx].volume = value as f32 / 127.0;
                    }
                    10 => { // Stereo Pan knob: scale 0-127 to -1.0 to 1.0
                        synth_locked.instruments[track_idx].pan = (value as f32 / 127.0) * 2.0 - 1.0;
                    }
                    73 => { // ADSR Attack: scale 0-127 to 0.001s-2.0s
                        let mut adsr = synth_locked.instruments[track_idx].adsr;
                        adsr.attack = 0.001 + (value as f32 / 127.0) * 1.999;
                        synth_locked.update_adsr(track_idx, adsr);
                    }
                    79 => { // ADSR Decay: scale 0-127 to 0.01s-3.0s
                        let mut adsr = synth_locked.instruments[track_idx].adsr;
                        adsr.decay = 0.01 + (value as f32 / 127.0) * 2.99;
                        synth_locked.update_adsr(track_idx, adsr);
                    }
                    75 => { // ADSR Sustain: scale 0-127 to 0.0-1.0
                        let mut adsr = synth_locked.instruments[track_idx].adsr;
                        adsr.sustain = value as f32 / 127.0;
                        synth_locked.update_adsr(track_idx, adsr);
                    }
                    72 => { // ADSR Release: scale 0-127 to 0.01s-4.0s
                        let mut adsr = synth_locked.instruments[track_idx].adsr;
                        adsr.release = 0.01 + (value as f32 / 127.0) * 3.99;
                        synth_locked.update_adsr(track_idx, adsr);
                    }
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

                // Send the ControlChange event to the recording channel so the App can log/monitor it!
                let _ = recording_sender.send(MidiRecordEvent::ControlChange { controller, value });
                
                let cc_name = get_cc_name(controller);
                let log_str = format!("Control Change: CC {} ({}) = {}", controller, cc_name, value);
                if let Ok(mut msg) = last_midi_message.lock() {
                    *msg = Some(log_str);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_to_name() {
        assert_eq!(pitch_to_name(60), "C4");
        assert_eq!(pitch_to_name(69), "A4");
        assert_eq!(pitch_to_name(72), "C5");
        assert_eq!(pitch_to_name(21), "A0");
        assert_eq!(pitch_to_name(70), "A#4");
    }

    #[test]
    fn test_keyboard_key_to_note() {
        assert_eq!(keyboard_key_to_note('a'), Some(60));
        assert_eq!(keyboard_key_to_note('s'), Some(62));
        assert_eq!(keyboard_key_to_note('w'), Some(61));
        assert_eq!(keyboard_key_to_note('x'), None);
        assert_eq!(keyboard_key_to_note(';'), Some(76));
    }

    #[test]
    fn test_get_cc_name() {
        assert_eq!(get_cc_name(7), "Volume Fader");
        assert_eq!(get_cc_name(10), "Stereo Pan");
        assert_eq!(get_cc_name(74), "Filter Cutoff (Freq)");
        assert_eq!(get_cc_name(99), "General Purpose Controller");
    }
}
