use std::sync::Arc;
use std::fs;
use crossterm::event::{KeyEvent, KeyCode, KeyModifiers, MouseEvent, MouseButton, MouseEventKind};
use ratatui::prelude::Rect;
use crate::sequencer::Sequencer;
use crate::synth::{SynthEngine, Waveform, InstrumentType};
use crate::audio::AudioEngine;
use crate::midi::{MidiManager, keyboard_key_to_note, MidiRecordEvent};
use crate::ui::{ActiveTab, ModalState, piano_roll::PianoRollState, synth_editor::SynthFocusField, mixer::MixerFocusField};
use crate::ui::visualizers::Sparkle;

pub struct App {
    pub active_tab: ActiveTab,
    pub sequencer: Sequencer,
    pub audio_engine: AudioEngine,
    pub midi_manager: MidiManager,
    
    // View states
    pub piano_roll_state: PianoRollState,
    pub focused_synth_field: SynthFocusField,
    pub focused_mixer_field: MixerFocusField,
    pub focused_mixer_track: usize,
    pub selected_device_idx: usize,
    pub modal_state: ModalState,
    pub sparkles: Vec<Sparkle>,

    // Recording helper states
    pub recording_rx: std::sync::mpsc::Receiver<MidiRecordEvent>,
    pub recording_tx: std::sync::mpsc::Sender<MidiRecordEvent>,
    pub active_recorded_notes: std::collections::HashMap<u8, (u32, u8)>, // note -> (start_tick, velocity)

    // Mouse grid sizing tracker
    pub grid_rendered_area: Rect,
}

impl App {
    pub fn new() -> Result<Self, anyhow::Error> {
        let audio_engine = AudioEngine::new()?;
        let midi_manager = MidiManager::new();

        let (recording_tx, recording_rx) = std::sync::mpsc::channel();

        Ok(Self {
            active_tab: ActiveTab::Sequencer,
            sequencer: Sequencer::new(),
            audio_engine,
            midi_manager,
            piano_roll_state: PianoRollState::default(),
            focused_synth_field: SynthFocusField::InstrumentType,
            focused_mixer_field: MixerFocusField::TrackVolume,
            focused_mixer_track: 0,
            selected_device_idx: 0,
            modal_state: ModalState {
                show_save: false,
                save_input: String::new(),
                show_load: false,
                load_files: Vec::new(),
                load_selected_idx: 0,
                toast_message: None,
                toast_ticks: 0,
            },
            sparkles: Vec::new(),
            recording_rx,
            recording_tx,
            active_recorded_notes: std::collections::HashMap::new(),
            grid_rendered_area: Rect::default(),
        })
    }

    pub fn trigger_toast(&mut self, msg: &str) {
        self.modal_state.toast_message = Some(msg.to_string());
        self.modal_state.toast_ticks = 40; // visible for 40 render frames (~1.5s)
    }

    pub fn trigger_key_press(&mut self, key_event: KeyEvent) -> bool {
        // --- MODAL HANDLING INPUT ---
        if self.modal_state.show_save {
            match key_event.code {
                KeyCode::Enter => {
                    self.save_project();
                }
                KeyCode::Esc => {
                    self.modal_state.show_save = false;
                }
                KeyCode::Char(c) => {
                    self.modal_state.save_input.push(c);
                }
                KeyCode::Backspace => {
                    self.modal_state.save_input.pop();
                }
                _ => {}
            }
            return false; // key consumed
        }

        if self.modal_state.show_load {
            match key_event.code {
                KeyCode::Up => {
                    if !self.modal_state.load_files.is_empty() {
                        if self.modal_state.load_selected_idx > 0 {
                            self.modal_state.load_selected_idx -= 1;
                        } else {
                            self.modal_state.load_selected_idx = self.modal_state.load_files.len() - 1;
                        }
                    }
                }
                KeyCode::Down => {
                    if !self.modal_state.load_files.is_empty() {
                        self.modal_state.load_selected_idx = (self.modal_state.load_selected_idx + 1) % self.modal_state.load_files.len();
                    }
                }
                KeyCode::Enter => {
                    self.load_project();
                }
                KeyCode::Esc => {
                    self.modal_state.show_load = false;
                }
                _ => {}
            }
            return false; // key consumed
        }

        // --- GLOBAL KEYBINDINGS ---
        // Tab switching: 1 to 5 keys
        if key_event.modifiers.is_empty() {
            match key_event.code {
                KeyCode::Char('1') => { self.active_tab = ActiveTab::Sequencer; return false; }
                KeyCode::Char('2') => { self.active_tab = ActiveTab::SynthEditor; return false; }
                KeyCode::Char('3') => { self.active_tab = ActiveTab::Mixer; return false; }
                KeyCode::Char('4') => {
                    self.midi_manager.refresh_ports();
                    self.active_tab = ActiveTab::Devices;
                    return false;
                }
                KeyCode::Char('5') => { self.active_tab = ActiveTab::Help; return false; }
                _ => {}
            }
        }

        // Global transport bindings
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                return true; // request app exit
            }
            KeyCode::Char(' ') => {
                if self.active_tab != ActiveTab::Sequencer {
                    self.toggle_play();
                } else {
                    // Inside Piano Roll, Space places a note, so we use 'P' or verify edit mode
                    // If no note exists under cursor, place one. If note exists, play/stop!
                    // Let's make Space toggle playback only if cursor note placing doesn't trigger
                    let cursor_pitch = self.piano_roll_state.cursor_pitch;
                    let cursor_tick = self.piano_roll_state.cursor_tick;
                    if self.sequencer.get_note_at(self.piano_roll_state.active_track_idx, cursor_pitch, cursor_tick).is_some() {
                        self.sequencer.remove_note_at(self.piano_roll_state.active_track_idx, cursor_pitch, cursor_tick);
                    } else {
                        self.sequencer.add_note(
                            self.piano_roll_state.active_track_idx,
                            cursor_pitch,
                            cursor_tick,
                            self.piano_roll_state.note_duration_default,
                            100, // default velocity
                        );
                        // Trigger synthetic sound preview of note placed
                        self.audio_engine.synth.lock().unwrap().note_on(self.piano_roll_state.active_track_idx, cursor_pitch, 0.8);
                        let synth_clone = Arc::clone(&self.audio_engine.synth);
                        let track_idx = self.piano_roll_state.active_track_idx;
                        let pitch = cursor_pitch;
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            synth_clone.lock().unwrap().note_off(track_idx, pitch);
                        });
                    }
                }
                return false;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.toggle_play();
                return false;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.toggle_record();
                return false;
            }
            KeyCode::Char('e') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.export_midi();
                return false;
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.modal_state.show_save = true;
                self.modal_state.save_input = String::new();
                return false;
            }
            KeyCode::Char('o') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_load_modal();
                return false;
            }
            _ => {}
        }

        // --- OFFLINE COMPUTER VIRTUAL PIANO KEYS PLAYING ---
        if let KeyCode::Char(c) = key_event.code {
            if let Some(note) = keyboard_key_to_note(c) {
                let track_idx = self.piano_roll_state.active_track_idx;
                
                // Audio note trigger
                self.audio_engine.synth.lock().unwrap().note_on(track_idx, note, 0.8);
                
                // If recording, register note-on event
                if self.sequencer.is_recording && self.sequencer.is_playing {
                    self.active_recorded_notes.insert(note, (self.sequencer.current_tick, 100));
                }

                // Auto note-off after a short duration to emulate key release
                let synth_clone = Arc::clone(&self.audio_engine.synth);
                let is_rec = self.sequencer.is_recording;
                let tx = self.recording_tx.clone();

                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    synth_clone.lock().unwrap().note_off(track_idx, note);
                    if is_rec {
                        let _ = tx.send(MidiRecordEvent::NoteOff { note });
                    }
                });

                return false;
            }
        }

        // --- TAB-SPECIFIC INPUT HANDLERS ---
        match self.active_tab {
            ActiveTab::Sequencer => self.handle_piano_roll_input(key_event),
            ActiveTab::SynthEditor => self.handle_synth_input(key_event),
            ActiveTab::Mixer => self.handle_mixer_input(key_event),
            ActiveTab::Devices => self.handle_device_input(key_event),
            ActiveTab::Help => {}
        }

        false
    }

    fn toggle_play(&mut self) {
        self.sequencer.is_playing = !self.sequencer.is_playing;
        if !self.sequencer.is_playing {
            self.audio_engine.synth.lock().unwrap().all_notes_off();
            self.sequencer.is_recording = false;
        }
        self.trigger_toast(if self.sequencer.is_playing { "▶ PLAYBACK STARTED" } else { "■ PLAYBACK STOPPED" });
    }

    fn toggle_record(&mut self) {
        self.sequencer.is_recording = !self.sequencer.is_recording;
        if self.sequencer.is_recording {
            self.sequencer.is_playing = true; // Auto start playing
            self.active_recorded_notes.clear();
        }
        self.trigger_toast(if self.sequencer.is_recording { "● RECORDING ARMED" } else { "○ RECORDING MUTED" });
    }

    pub fn process_recording_events(&mut self) {
        // Dequeue and process MIDI events recorded on the fly
        while let Ok(event) = self.recording_rx.try_recv() {
            let track_idx = self.piano_roll_state.active_track_idx;
            match event {
                MidiRecordEvent::NoteOn { note, velocity } => {
                    if self.sequencer.is_recording && self.sequencer.is_playing {
                        self.active_recorded_notes.insert(note, (self.sequencer.current_tick, velocity));
                    }
                }
                MidiRecordEvent::NoteOff { note } => {
                    if let Some((start_tick, velocity)) = self.active_recorded_notes.remove(&note) {
                        let duration = self.sequencer.current_tick.saturating_sub(start_tick).max(1);
                        self.sequencer.add_note(track_idx, note, start_tick, duration, velocity);
                    }
                }
            }
        }
    }

    // Piano roll interactive navigation and editing
    fn handle_piano_roll_input(&mut self, key_event: KeyEvent) {
        let snap = self.sequencer.snap_division;

        match key_event.code {
            KeyCode::Up => {
                // Shift transposes selected note pitch, normal arrow moves cursor
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    if let Some(note_idx) = self.piano_roll_state.selected_note_idx {
                        let active_track = &mut self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                        if let Some(note) = active_track.notes.get_mut(note_idx) {
                            if note.pitch < 127 {
                                note.pitch += 1;
                                self.piano_roll_state.cursor_pitch = note.pitch;
                            }
                        }
                    }
                } else if self.piano_roll_state.cursor_pitch < 127 {
                    self.piano_roll_state.cursor_pitch += 1;
                }
            }
            KeyCode::Down => {
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    if let Some(note_idx) = self.piano_roll_state.selected_note_idx {
                        let active_track = &mut self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                        if let Some(note) = active_track.notes.get_mut(note_idx) {
                            if note.pitch > 0 {
                                note.pitch -= 1;
                                self.piano_roll_state.cursor_pitch = note.pitch;
                            }
                        }
                    }
                } else if self.piano_roll_state.cursor_pitch > 0 {
                    self.piano_roll_state.cursor_pitch -= 1;
                }
            }
            KeyCode::Left => {
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shrink selected note duration
                    if let Some(note_idx) = self.piano_roll_state.selected_note_idx {
                        let active_track = &mut self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                        if let Some(note) = active_track.notes.get_mut(note_idx) {
                            if note.duration_ticks > snap {
                                note.duration_ticks -= snap;
                            }
                        }
                    }
                } else if self.piano_roll_state.cursor_tick >= snap {
                    self.piano_roll_state.cursor_tick -= snap;
                } else {
                    self.piano_roll_state.cursor_tick = 0;
                }
            }
            KeyCode::Right => {
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    // Grow selected note duration
                    if let Some(note_idx) = self.piano_roll_state.selected_note_idx {
                        let active_track = &mut self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                        if let Some(note) = active_track.notes.get_mut(note_idx) {
                            note.duration_ticks += snap;
                        }
                    }
                } else if self.piano_roll_state.cursor_tick + snap < self.sequencer.max_ticks {
                    self.piano_roll_state.cursor_tick += snap;
                }
            }
            KeyCode::Delete | KeyCode::Backspace => {
                let pitch = self.piano_roll_state.cursor_pitch;
                let tick = self.piano_roll_state.cursor_tick;
                let removed = self.sequencer.remove_note_at(self.piano_roll_state.active_track_idx, pitch, tick);
                if removed {
                    self.piano_roll_state.selected_note_idx = None;
                }
            }
            KeyCode::Tab => {
                // Cycles active track editor
                self.piano_roll_state.active_track_idx = (self.piano_roll_state.active_track_idx + 1) % 4;
            }
            KeyCode::BackTab => {
                if self.piano_roll_state.active_track_idx > 0 {
                    self.piano_roll_state.active_track_idx -= 1;
                } else {
                    self.piano_roll_state.active_track_idx = 3;
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Adjust snapped grid size default note length
                self.piano_roll_state.note_duration_default += 1;
                self.trigger_toast(format!("DEFAULT NOTE LENGTH: {} TICKS", self.piano_roll_state.note_duration_default).as_str());
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                if self.piano_roll_state.note_duration_default > 1 {
                    self.piano_roll_state.note_duration_default -= 1;
                    self.trigger_toast(format!("DEFAULT NOTE LENGTH: {} TICKS", self.piano_roll_state.note_duration_default).as_str());
                }
            }
            KeyCode::Char('[') => {
                let snaps = [1, 2, 4, 8, 16];
                let current = self.sequencer.snap_division;
                if let Some(pos) = snaps.iter().position(|&x| x == current) {
                    if pos < snaps.len() - 1 {
                        self.sequencer.snap_division = snaps[pos + 1];
                        self.piano_roll_state.cursor_tick = (self.piano_roll_state.cursor_tick / self.sequencer.snap_division) * self.sequencer.snap_division;
                        let msg = format!("GRID SNAP: {}", match self.sequencer.snap_division {
                            1 => "1/16",
                            2 => "1/8",
                            4 => "1/4",
                            8 => "1/2",
                            _ => "1 Bar",
                        });
                        self.trigger_toast(&msg);
                    }
                }
            }
            KeyCode::Char(']') => {
                let snaps = [1, 2, 4, 8, 16];
                let current = self.sequencer.snap_division;
                if let Some(pos) = snaps.iter().position(|&x| x == current) {
                    if pos > 0 {
                        self.sequencer.snap_division = snaps[pos - 1];
                        self.piano_roll_state.cursor_tick = (self.piano_roll_state.cursor_tick / self.sequencer.snap_division) * self.sequencer.snap_division;
                        let msg = format!("GRID SNAP: {}", match self.sequencer.snap_division {
                            1 => "1/16",
                            2 => "1/8",
                            4 => "1/4",
                            8 => "1/2",
                            _ => "1 Bar",
                        });
                        self.trigger_toast(&msg);
                    }
                }
            }
            _ => {}
        }

        // Highlight selected note under the cursor (if any exists)
        let pitch = self.piano_roll_state.cursor_pitch;
        let tick = self.piano_roll_state.cursor_tick;
        let active_track = &self.sequencer.tracks[self.piano_roll_state.active_track_idx];
        self.piano_roll_state.selected_note_idx = active_track.notes.iter().position(|n| {
            n.pitch == pitch && n.start_tick <= tick && tick < n.start_tick + n.duration_ticks
        });
    }

    // Mouse grid click processing
    pub fn handle_mouse_click(&mut self, mouse: MouseEvent) {
        if self.active_tab != ActiveTab::Sequencer {
            return;
        }

        let area = self.grid_rendered_area;
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Check if mouse event is click inside the grid coordinates
        let m_col = mouse.column;
        let m_row = mouse.row;

        if m_col >= area.x + 1 && m_col < area.x + area.width - 1 &&
           m_row >= area.y + 1 && m_row < area.y + area.height - 1 {
            
            // Map character grid coordinates
            let clicked_x = (m_col - area.x - 1) as u32;
            let clicked_y = (m_row - area.y - 1) as u8;

            let grid_height = area.height as u8 - 2;

            let tick = self.piano_roll_state.scroll_tick + clicked_x;
            let pitch = self.piano_roll_state.scroll_pitch + grid_height - 1 - clicked_y;

            if tick >= self.sequencer.max_ticks {
                return;
            }

            self.piano_roll_state.cursor_tick = tick;
            self.piano_roll_state.cursor_pitch = pitch;

            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    // If note exists, select it. If not, add note!
                    let active_track = &self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                    let note_exists = active_track.notes.iter().any(|n| {
                        n.pitch == pitch && n.start_tick <= tick && tick < n.start_tick + n.duration_ticks
                    });

                    if !note_exists {
                        self.sequencer.add_note(
                            self.piano_roll_state.active_track_idx,
                            pitch,
                            tick,
                            self.piano_roll_state.note_duration_default,
                            100,
                        );
                        // Trigger synthetic sound preview of note placed
                        self.audio_engine.synth.lock().unwrap().note_on(self.piano_roll_state.active_track_idx, pitch, 0.8);
                        let synth_clone = Arc::clone(&self.audio_engine.synth);
                        let track_idx = self.piano_roll_state.active_track_idx;
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            synth_clone.lock().unwrap().note_off(track_idx, pitch);
                        });
                    }

                    // Highlight selection
                    let active_track = &self.sequencer.tracks[self.piano_roll_state.active_track_idx];
                    self.piano_roll_state.selected_note_idx = active_track.notes.iter().position(|n| {
                        n.pitch == pitch && n.start_tick <= tick && tick < n.start_tick + n.duration_ticks
                    });
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    // Right-click deletes
                    self.sequencer.remove_note_at(self.piano_roll_state.active_track_idx, pitch, tick);
                    self.piano_roll_state.selected_note_idx = None;
                }
                _ => {}
            }
        }
    }

    // Synthesizer knob/parameter editor controls
    fn handle_synth_input(&mut self, key_event: KeyEvent) {
        let track_idx = self.piano_roll_state.active_track_idx;
        let focused_field = self.focused_synth_field;

        match key_event.code {
            KeyCode::Tab => {
                self.focused_synth_field = self.focused_synth_field.next();
            }
            KeyCode::BackTab => {
                self.focused_synth_field = self.focused_synth_field.prev();
            }
            KeyCode::Right | KeyCode::Up => {
                let toast = {
                    let mut synth_locked = self.audio_engine.synth.lock().unwrap();
                    let inst = synth_locked.instruments[track_idx].clone();
                    Self::adjust_synth_field(focused_field, &mut synth_locked, &inst, track_idx, 1.0)
                };
                if let Some(msg) = toast {
                    self.trigger_toast(&msg);
                }
            }
            KeyCode::Left | KeyCode::Down => {
                let toast = {
                    let mut synth_locked = self.audio_engine.synth.lock().unwrap();
                    let inst = synth_locked.instruments[track_idx].clone();
                    Self::adjust_synth_field(focused_field, &mut synth_locked, &inst, track_idx, -1.0)
                };
                if let Some(msg) = toast {
                    self.trigger_toast(&msg);
                }
            }
            _ => {}
        }
    }

    fn adjust_synth_field(
        focused_field: SynthFocusField,
        synth: &mut SynthEngine,
        inst: &crate::synth::InstrumentConfig,
        track_idx: usize,
        direction: f32,
    ) -> Option<String> {
        match focused_field {
            SynthFocusField::InstrumentType => {
                let types = [
                    InstrumentType::Subtractive,
                    InstrumentType::Fm,
                    InstrumentType::Plucked,
                    InstrumentType::Drum,
                    InstrumentType::Additive,
                    InstrumentType::Supersaw,
                ];
                let cur_idx = types.iter().position(|&t| t == inst.inst_type).unwrap_or(0);
                let next_idx = if direction > 0.0 {
                    (cur_idx + 1) % types.len()
                } else if cur_idx == 0 {
                    types.len() - 1
                } else {
                    cur_idx - 1
                };
                
                // Swap instrument engine type
                let next_type = types[next_idx];
                synth.instruments[track_idx] = crate::synth::InstrumentConfig::new(&inst.name, next_type);
                Some(format!("TRACK ENGINE MOUNTED: {:?}", next_type))
            }
            SynthFocusField::Waveform => {
                if inst.inst_type == InstrumentType::Subtractive {
                    let waves = [Waveform::Sine, Waveform::Square, Waveform::Saw, Waveform::Triangle];
                    let cur_idx = waves.iter().position(|&w| w == inst.waveform).unwrap_or(0);
                    let next_idx = if direction > 0.0 {
                        (cur_idx + 1) % waves.len()
                    } else if cur_idx == 0 {
                        waves.len() - 1
                    } else {
                        cur_idx - 1
                    };
                    synth.set_waveform(track_idx, waves[next_idx]);
                }
                None
            }
            SynthFocusField::Attack => {
                let mut adsr = inst.adsr;
                adsr.attack = (adsr.attack + direction * 0.05).clamp(0.001, 2.0);
                synth.update_adsr(track_idx, adsr);
                None
            }
            SynthFocusField::Decay => {
                let mut adsr = inst.adsr;
                adsr.decay = (adsr.decay + direction * 0.05).clamp(0.001, 2.0);
                synth.update_adsr(track_idx, adsr);
                None
            }
            SynthFocusField::Sustain => {
                let mut adsr = inst.adsr;
                adsr.sustain = (adsr.sustain + direction * 0.05).clamp(0.0, 1.0);
                synth.update_adsr(track_idx, adsr);
                None
            }
            SynthFocusField::Release => {
                let mut adsr = inst.adsr;
                adsr.release = (adsr.release + direction * 0.05).clamp(0.001, 3.0);
                synth.update_adsr(track_idx, adsr);
                None
            }
            SynthFocusField::FmRatio => {
                if inst.inst_type == InstrumentType::Fm || inst.inst_type == InstrumentType::Additive || inst.inst_type == InstrumentType::Supersaw {
                    let (min, max, step) = match inst.inst_type {
                        InstrumentType::Fm => (0.25, 8.0, 0.25),
                        InstrumentType::Additive => (0.25, 4.0, 0.1),
                        InstrumentType::Supersaw => (0.0, 1.0, 0.05),
                        _ => (0.0, 1.0, 0.1),
                    };
                    let val = (inst.fm_ratio + direction * step).clamp(min, max);
                    synth.instruments[track_idx].fm_ratio = val;
                }
                None
            }
            SynthFocusField::FmIndex => {
                if inst.inst_type == InstrumentType::Fm || inst.inst_type == InstrumentType::Additive || inst.inst_type == InstrumentType::Supersaw {
                    let (min, max, step) = match inst.inst_type {
                        InstrumentType::Fm => (0.0, 20.0, 0.5),
                        InstrumentType::Additive => (0.0, 20.0, 0.5),
                        InstrumentType::Supersaw => (2.0, 6.0, 1.0),
                        _ => (0.0, 1.0, 0.1),
                    };
                    let val = (inst.fm_index + direction * step).clamp(min, max);
                    synth.instruments[track_idx].fm_index = val;
                }
                None
            }
            SynthFocusField::FilterCutoff => {
                let val = (synth.filter_cutoff + direction * 100.0).clamp(50.0, 8000.0);
                synth.update_filter(val, synth.filter_resonance);
                None
            }
            SynthFocusField::FilterResonance => {
                let val = (synth.filter_resonance + direction * 0.1).clamp(0.1, 5.0);
                synth.update_filter(synth.filter_cutoff, val);
                None
            }
            SynthFocusField::DelayTime => {
                let val = (synth.delay_time + direction * 0.05).clamp(0.05, 1.0);
                synth.delay_time = val;
                None
            }
            SynthFocusField::DelayFeedback => {
                let val = (synth.delay_feedback + direction * 0.05).clamp(0.0, 0.95);
                synth.delay_feedback = val;
                None
            }
        }
    }

    // Mixer strip controls
    fn handle_mixer_input(&mut self, key_event: KeyEvent) {
        let mut synth_locked = self.audio_engine.synth.lock().unwrap();

        match key_event.code {
            KeyCode::Tab => {
                self.focused_mixer_track = (self.focused_mixer_track + 1) % 4;
            }
            KeyCode::BackTab => {
                if self.focused_mixer_track > 0 {
                    self.focused_mixer_track -= 1;
                } else {
                    self.focused_mixer_track = 3;
                }
            }
            KeyCode::Up => {
                // Adjust focused parameter: Volume
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.volume = (inst.volume + 0.05).clamp(0.0, 1.2); // allow slight boost
                synth_locked.instruments[self.focused_mixer_track].volume = inst.volume;
            }
            KeyCode::Down => {
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.volume = (inst.volume - 0.05).clamp(0.0, 1.2);
                synth_locked.instruments[self.focused_mixer_track].volume = inst.volume;
            }
            KeyCode::Left => {
                // Adjust panning fader
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.pan = (inst.pan - 0.1).clamp(-1.0, 1.0);
                synth_locked.instruments[self.focused_mixer_track].pan = inst.pan;
            }
            KeyCode::Right => {
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.pan = (inst.pan + 0.1).clamp(-1.0, 1.0);
                synth_locked.instruments[self.focused_mixer_track].pan = inst.pan;
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                // Toggle Mute
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.mute = !inst.mute;
                synth_locked.instruments[self.focused_mixer_track].mute = inst.mute;
                drop(synth_locked);
                self.trigger_toast(if inst.mute { "TRACK MUTED" } else { "TRACK UNMUTED" });
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Toggle Solo
                let mut inst = synth_locked.instruments[self.focused_mixer_track].clone();
                inst.solo = !inst.solo;
                synth_locked.instruments[self.focused_mixer_track].solo = inst.solo;
                drop(synth_locked);
                self.trigger_toast(if inst.solo { "TRACK SOLO ACTIVE" } else { "TRACK SOLO INACTIVE" });
            }
            _ => {}
        }
    }

    // Devices port selection manager controls
    fn handle_device_input(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => {
                if !self.midi_manager.ports.is_empty() {
                    if self.selected_device_idx > 0 {
                        self.selected_device_idx -= 1;
                    } else {
                        self.selected_device_idx = self.midi_manager.ports.len() - 1;
                    }
                }
            }
            KeyCode::Down => {
                if !self.midi_manager.ports.is_empty() {
                    self.selected_device_idx = (self.selected_device_idx + 1) % self.midi_manager.ports.len();
                }
            }
            KeyCode::Enter => {
                if !self.midi_manager.ports.is_empty() {
                    if self.midi_manager.is_connected() && self.midi_manager.selected_port_idx == Some(self.selected_device_idx) {
                        self.midi_manager.disconnect();
                        self.trigger_toast("✗ MIDI DEVICE DISCONNECTED");
                    } else {
                        let synth_clone = Arc::clone(&self.audio_engine.synth);
                        let sender = self.recording_tx.clone();
                        match self.midi_manager.connect(self.selected_device_idx, synth_clone, sender) {
                            Ok(_) => {
                                let name = self.midi_manager.connected_port_name.clone().unwrap_or("Midi Port".to_string());
                                self.trigger_toast(format!("✔ CONNECTED: {}", name).as_str());
                            }
                            Err(e) => {
                                self.trigger_toast(format!("ERROR: {}", e).as_str());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // --- SAVE AND LOAD PROCEDURES ---
    fn save_project(&mut self) {
        if self.modal_state.save_input.is_empty() {
            return;
        }

        let mut filename = self.modal_state.save_input.trim().to_string();
        if !filename.ends_with(".json") {
            filename.push_str(".json");
        }

        // Aggregate DAW parameters for storage
        #[derive(serde::Serialize)]
        struct ProjectState {
            sequencer: Sequencer,
            synth_filter_cutoff: f32,
            synth_filter_resonance: f32,
            synth_delay_time: f32,
            synth_delay_feedback: f32,
            instruments: Vec<crate::synth::InstrumentConfig>,
        }

        let synth_locked = self.audio_engine.synth.lock().unwrap();
        let state = ProjectState {
            sequencer: self.sequencer.clone(),
            synth_filter_cutoff: synth_locked.filter_cutoff,
            synth_filter_resonance: synth_locked.filter_resonance,
            synth_delay_time: synth_locked.delay_time,
            synth_delay_feedback: synth_locked.delay_feedback,
            instruments: synth_locked.instruments.clone(),
        };
        drop(synth_locked);

        match serde_json::to_string_pretty(&state) {
            Ok(json) => {
                if let Err(e) = fs::write(&filename, json) {
                    self.trigger_toast(format!("FS ERROR: {:?}", e).as_str());
                } else {
                    self.modal_state.show_save = false;
                    self.trigger_toast(format!("✔ SAVED: {}", filename).as_str());
                }
            }
            Err(e) => {
                self.trigger_toast(format!("SERIALIZE ERROR: {:?}", e).as_str());
            }
        }
    }

    fn open_load_modal(&mut self) {
        // Scan directory for .json files
        let mut json_files = Vec::new();
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries {
                if let Ok(e) = entry {
                    let path = e.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            json_files.push(name.to_string());
                        }
                    }
                }
            }
        }

        self.modal_state.load_files = json_files;
        self.modal_state.load_selected_idx = 0;
        self.modal_state.show_load = true;
    }

    fn load_project(&mut self) {
        if self.modal_state.load_files.is_empty() {
            return;
        }

        let filename = &self.modal_state.load_files[self.modal_state.load_selected_idx];

        #[derive(serde::Deserialize)]
        struct ProjectState {
            sequencer: Sequencer,
            synth_filter_cutoff: f32,
            synth_filter_resonance: f32,
            synth_delay_time: f32,
            synth_delay_feedback: f32,
            instruments: Vec<crate::synth::InstrumentConfig>,
        }

        match fs::read_to_string(filename) {
            Ok(content) => {
                match serde_json::from_str::<ProjectState>(&content) {
                    Ok(state) => {
                        self.sequencer = state.sequencer;
                        
                        let mut synth_locked = self.audio_engine.synth.lock().unwrap();
                        synth_locked.instruments = state.instruments;
                        synth_locked.update_filter(state.synth_filter_cutoff, state.synth_filter_resonance);
                        synth_locked.delay_time = state.synth_delay_time;
                        synth_locked.delay_feedback = state.synth_delay_feedback;
                        drop(synth_locked);

                        self.modal_state.show_load = false;
                        self.trigger_toast(format!("✔ LOADED: {}", filename).as_str());
                    }
                    Err(e) => {
                        self.trigger_toast(format!("PARSE ERROR: {:?}", e).as_str());
                    }
                }
            }
            Err(e) => {
                self.trigger_toast(format!("FS ERROR: {:?}", e).as_str());
            }
        }
    }

    // Export track note sequencer data to Standard MIDI File format
    fn export_midi(&mut self) {
        let export_name = "midi-tui-export.mid";
        match self.sequencer.export_to_midi(export_name) {
            Ok(_) => {
                self.trigger_toast(format!("✔ EXPORTED MIDI: {}", export_name).as_str());
            }
            Err(e) => {
                self.trigger_toast(format!("EXPORT ERROR: {:?}", e).as_str());
            }
        }
    }
}
