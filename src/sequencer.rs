use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use crate::synth::SynthEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u32,
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub notes: Vec<Note>,
    pub color_hue: u16, // HSL hue for TUI rendering
}

impl Track {
    pub fn new(name: &str, hue: u16) -> Self {
        Self {
            name: name.to_string(),
            notes: Vec::new(),
            color_hue: hue,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequencer {
    pub tracks: Vec<Track>,
    pub bpm: f32,
    pub current_tick: u32,
    pub max_ticks: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub is_playing: bool,
    pub is_recording: bool,
    pub ticks_per_beat: u32, // e.g. 4 ticks per beat (16th notes)
    pub snap_division: u32,  // grid snap division (usually 1 tick = 1/16th)
}

impl Sequencer {
    pub fn new() -> Self {
        let tracks = vec![
            Track::new("Track 1: Lead", 280),  // Purple
            Track::new("Track 2: Bass", 200),  // Cyan
            Track::new("Track 3: Pad", 120),   // Green
            Track::new("Track 4: Drums", 350),  // Magenta
        ];

        Self {
            tracks,
            bpm: 120.0,
            current_tick: 0,
            max_ticks: 64, // 4 measures of 4/4 time (16 beats * 4 ticks = 64 ticks)
            loop_start: 0,
            loop_end: 64,
            is_playing: false,
            is_recording: false,
            ticks_per_beat: 4,
            snap_division: 1,
        }
    }

    pub fn add_note(&mut self, track_idx: usize, pitch: u8, start_tick: u32, duration_ticks: u32, velocity: u8) {
        if track_idx >= self.tracks.len() {
            return;
        }

        // Avoid exact pitch overlap at the same start tick
        self.remove_note_at(track_idx, pitch, start_tick);

        let note = Note {
            pitch,
            velocity,
            start_tick,
            duration_ticks,
        };

        self.tracks[track_idx].notes.push(note);
    }

    pub fn remove_note_at(&mut self, track_idx: usize, pitch: u8, tick: u32) -> bool {
        if track_idx >= self.tracks.len() {
            return false;
        }
        let notes = &mut self.tracks[track_idx].notes;
        let initial_len = notes.len();
        notes.retain(|n| !(n.pitch == pitch && n.start_tick <= tick && tick < n.start_tick + n.duration_ticks));
        notes.len() < initial_len
    }


    pub fn get_note_at(&self, track_idx: usize, pitch: u8, tick: u32) -> Option<&Note> {
        if track_idx >= self.tracks.len() {
            return None;
        }
        self.tracks[track_idx].notes.iter().find(|n| {
            n.pitch == pitch && n.start_tick <= tick && tick < n.start_tick + n.duration_ticks
        })
    }


    pub fn tick_duration(&self) -> std::time::Duration {
        // Seconds per beat = 60.0 / bpm
        // Seconds per tick = (60.0 / bpm) / ticks_per_beat
        let secs_per_tick = 15.0 / self.bpm;
        std::time::Duration::from_secs_f32(secs_per_tick)
    }

    pub fn advance_tick(&mut self, synth: &Arc<Mutex<SynthEngine>>) {
        let prev_tick = self.current_tick;
        self.current_tick += 1;

        if self.current_tick >= self.loop_end {
            self.current_tick = self.loop_start;
            // Stop any hanging notes on loop wraparound
            let mut synth_locked = synth.lock().unwrap();
            synth_locked.all_notes_off();
        }

        // Trigger notes for the current tick
        let mut synth_locked = synth.lock().unwrap();
        
        // 1. Release notes ending at the current tick
        for (track_idx, track) in self.tracks.iter().enumerate() {
            for note in &track.notes {
                if note.start_tick + note.duration_ticks == self.current_tick 
                   || (self.current_tick == self.loop_start && note.start_tick + note.duration_ticks >= prev_tick + 1)
                {
                    synth_locked.note_off(track_idx, note.pitch);
                }
            }
        }

        // 2. Trigger notes starting at the current tick
        for (track_idx, track) in self.tracks.iter().enumerate() {
            for note in &track.notes {
                if note.start_tick == self.current_tick {
                    synth_locked.note_on(track_idx, note.pitch, note.velocity as f32 / 127.0);
                }
            }
        }
    }

    // Capture standard MIDI files (.mid) export using `midly`
    pub fn export_to_midi(&self, filename: &str) -> Result<(), anyhow::Error> {
        use midly::{Header, Format, Timing, TrackEvent, TrackEventKind, MetaMessage, MidiMessage};

        let mut smf = midly::Smf::new(Header::new(
            Format::Parallel,
            Timing::Metrical(192.into()), // 192 ticks per quarter note
        ));

        // Metrical tempo is beats, our ticks_per_beat = 4 (16th notes)
        // 192 PPQ means 1 quarter note = 192 ticks in MIDI, or 48 ticks per 16th note.
        let midi_ticks_per_seq_tick: u32 = 48; 

        // Add 4 tracks
        for (track_idx, seq_track) in self.tracks.iter().enumerate() {
            let mut midi_track = Vec::new();
            
            // Collect all note events (NoteOn and NoteOff)
            struct Event {
                tick: u32,
                pitch: u8,
                velocity: u8,
                is_on: bool,
            }

            let mut events = Vec::new();
            for note in &seq_track.notes {
                events.push(Event {
                    tick: note.start_tick * midi_ticks_per_seq_tick,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    is_on: true,
                });
                events.push(Event {
                    tick: (note.start_tick + note.duration_ticks) * midi_ticks_per_seq_tick,
                    pitch: note.pitch,
                    velocity: 0,
                    is_on: false,
                });
            }

            // Sort events by tick time
            events.sort_by_key(|e| e.tick);

            // Convert to MIDI delta-times
            let mut last_tick = 0;
            
            // Set track name metadata at start
            midi_track.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(seq_track.name.as_bytes())),
            });

            // If it is the first track, add tempo metadata
            if track_idx == 0 {
                let tempo_val = (60_000_000.0 / self.bpm) as u32; // microseconds per beat
                midi_track.push(TrackEvent {
                    delta: 0.into(),
                    kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo_val.into())),
                });
            }

            for ev in events {
                let delta = ev.tick - last_tick;
                last_tick = ev.tick;

                let midi_ev = if ev.is_on {
                    MidiMessage::NoteOn {
                        key: ev.pitch.into(),
                        vel: ev.velocity.into(),
                    }
                } else {
                    MidiMessage::NoteOff {
                        key: ev.pitch.into(),
                        vel: 0.into(),
                    }
                };

                midi_track.push(TrackEvent {
                    delta: delta.into(),
                    kind: TrackEventKind::Midi {
                        channel: (track_idx as u8).into(),
                        message: midi_ev,
                    },
                });
            }

            // End of track meta message
            midi_track.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            });

            smf.tracks.push(midi_track);
        }

        let mut buffer = Vec::new();
        smf.write(&mut buffer).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        
        let mut file = std::fs::File::create(filename)?;
        use std::io::Write;
        file.write_all(&buffer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequencer_new() {
        let seq = Sequencer::new();
        assert_eq!(seq.tracks.len(), 4);
        assert_eq!(seq.bpm, 120.0);
        assert_eq!(seq.current_tick, 0);
        assert_eq!(seq.max_ticks, 64);
        assert_eq!(seq.is_playing, false);
        assert_eq!(seq.is_recording, false);
        assert_eq!(seq.ticks_per_beat, 4);
        assert_eq!(seq.snap_division, 1);
    }

    #[test]
    fn test_add_and_remove_notes() {
        let mut seq = Sequencer::new();
        
        // Ensure initial state is empty
        assert_eq!(seq.tracks[0].notes.len(), 0);

        // Add a note
        seq.add_note(0, 60, 0, 4, 80);
        assert_eq!(seq.tracks[0].notes.len(), 1);
        assert_eq!(seq.tracks[0].notes[0], Note { pitch: 60, velocity: 80, start_tick: 0, duration_ticks: 4 });

        // Retrieve the note
        assert!(seq.get_note_at(0, 60, 0).is_some());
        assert!(seq.get_note_at(0, 60, 2).is_some()); // Should be found since duration is 4
        assert!(seq.get_note_at(0, 60, 4).is_none()); // Outside duration

        // Try to add overlapping note at same start tick -> should replace
        seq.add_note(0, 60, 0, 8, 100);
        assert_eq!(seq.tracks[0].notes.len(), 1);
        assert_eq!(seq.tracks[0].notes[0].duration_ticks, 8);
        assert_eq!(seq.tracks[0].notes[0].velocity, 100);

        // Remove the note
        let removed = seq.remove_note_at(0, 60, 4); // Inside the note's duration
        assert!(removed);
        assert_eq!(seq.tracks[0].notes.len(), 0);
    }

    #[test]
    fn test_tick_advancement() {
        let mut seq = Sequencer::new();
        seq.current_tick = 63;
        seq.is_playing = true;
        
        // Wrap around test
        seq.current_tick = (seq.current_tick + 1) % seq.max_ticks;
        assert_eq!(seq.current_tick, 0);
    }
}
