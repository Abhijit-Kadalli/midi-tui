pub mod piano_roll;
pub mod synth_editor;
pub mod mixer;
pub mod device_manager;
pub mod visualizers;

use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::{Arc, Mutex};
use crate::sequencer::Sequencer;
use crate::synth::SynthEngine;
use crate::audio::VisualizerBuffer;
use crate::midi::MidiManager;
use crate::config::*;

use self::piano_roll::{PianoRollState, draw_piano_roll};
use self::synth_editor::{SynthFocusField, draw_synth_editor};
use self::mixer::{MixerFocusField, draw_mixer};
use self::device_manager::draw_device_manager;
use self::visualizers::{draw_waveform_visualizer, draw_keyboard_visualizer, Sparkle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveTab {
    Sequencer = 0,
    SynthEditor = 1,
    Mixer = 2,
    Devices = 3,
    Help = 4,
}

pub struct ModalState {
    pub show_save: bool,
    pub save_input: String,
    pub show_load: bool,
    pub load_files: Vec<String>,
    pub load_selected_idx: usize,
    pub toast_message: Option<String>,
    pub toast_ticks: usize, // durations before auto-clear
}

pub fn draw_main_ui(
    f: &mut Frame,
    // System states
    sequencer: &Sequencer,
    synth: &Arc<Mutex<SynthEngine>>,
    midi_manager: &MidiManager,
    vis_buf: &Arc<Mutex<VisualizerBuffer>>,
    // View states
    active_tab: ActiveTab,
    piano_roll_state: &mut PianoRollState,
    focused_synth_field: SynthFocusField,
    focused_mixer_field: MixerFocusField,
    focused_mixer_track: usize,
    selected_device_idx: usize,
    modal_state: &mut ModalState,
    sparkles: &mut Vec<Sparkle>,
) {
    // Background filling
    let base_block = Block::default().bg(COLOR_BASE);
    f.render_widget(base_block, f.size());

    // 1. Overall screen splitter
    // - Header (3 lines)
    // - Body Workspace (remainder)
    // - Footer visualizers (6 lines at bottom)
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Workspace
            Constraint::Length(10),  // Waveform + Keyboard
        ])
        .split(f.size());

    let header_area = main_chunks[0];
    let body_area = main_chunks[1];
    let footer_area = main_chunks[2];

    // --- RENDER HEADER ---
    draw_header(f, header_area, active_tab, sequencer);

    // --- RENDER FOOTER VISUALIZERS ---
    let footer_splits = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(45), // Waveform
            Constraint::Percentage(55), // Keyboard
        ])
        .split(footer_area);

    draw_waveform_visualizer(f, footer_splits[0], vis_buf);
    draw_keyboard_visualizer(f, footer_splits[1], synth, sparkles);

    // --- RENDER MAIN BODY ---
    match active_tab {
        ActiveTab::Sequencer => {
            draw_piano_roll(f, body_area, sequencer, piano_roll_state);
        }
        ActiveTab::SynthEditor => {
            let mut synth_locked = synth.lock().unwrap();
            draw_synth_editor(
                f,
                body_area,
                &mut synth_locked,
                piano_roll_state.active_track_idx,
                focused_synth_field,
            );
        }
        ActiveTab::Mixer => {
            let mut synth_locked = synth.lock().unwrap();
            draw_mixer(
                f,
                body_area,
                &mut synth_locked,
                focused_mixer_track,
                focused_mixer_field,
            );
        }
        ActiveTab::Devices => {
            draw_device_manager(f, body_area, midi_manager, selected_device_idx);
        }
        ActiveTab::Help => {
            draw_help_screen(f, body_area);
        }
    }

    // --- RENDER DIALOG MODALS ---
    if modal_state.show_save {
        draw_save_modal(f, modal_state.save_input.as_str());
    } else if modal_state.show_load {
        draw_load_modal(f, &modal_state.load_files, modal_state.load_selected_idx);
    }

    // --- RENDER TOAST MESSAGE ---
    if let Some(ref msg) = modal_state.toast_message {
        draw_toast_message(f, msg);
        if modal_state.toast_ticks > 0 {
            modal_state.toast_ticks -= 1;
        } else {
            modal_state.toast_message = None;
        }
    }
}

fn draw_header(f: &mut Frame, area: Rect, active_tab: ActiveTab, sequencer: &Sequencer) {
    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(COLOR_SURFACE0));

    f.render_widget(header_block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(16),     // Logo title
            Constraint::Length(45),     // Tab selectors
            Constraint::Min(20),        // Transport and indicators
        ])
        .split(area);

    // 1. Logo
    let logo = Paragraph::new(Span::styled(" MIDI-TUI ", Style::default().fg(COLOR_MAUVE).bold()))
        .alignment(Alignment::Left);
    f.render_widget(logo, chunks[0]);

    // 2. Tabs: 1-5 navigation
    let tab_titles = ["1:SEQ", "2:SYNTH", "3:MIXER", "4:DEVICES", "5:HELP"];
    let mut spans = Vec::new();
    for (i, title) in tab_titles.iter().enumerate() {
        let is_active = active_tab as usize == i;
        if is_active {
            spans.push(Span::styled(format!("  [{}]  ", title), Style::default().bg(COLOR_MAUVE).fg(COLOR_BASE).bold()));
        } else {
            spans.push(Span::styled(format!("   {}   ", title), Style::default().fg(COLOR_TEXT)));
        }
    }
    let tabs_widget = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    f.render_widget(tabs_widget, chunks[1]);

    // 3. Transport Indicators
    let mut transport_spans = Vec::new();

    // Loop point
    transport_spans.push(Span::styled(
        format!(" 🔁 Loop: [{} - {}] ", sequencer.loop_start, sequencer.loop_end),
        Style::default().fg(COLOR_BLUE),
    ));

    // Playback state
    if sequencer.is_playing {
        transport_spans.push(Span::styled(" ▶ PLAYING ", Style::default().bg(COLOR_GREEN).fg(COLOR_BASE).bold()));
    } else {
        transport_spans.push(Span::styled(" ■ STOPPED ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_TEXT)));
    }

    transport_spans.push(Span::raw(" "));

    // Record state
    if sequencer.is_recording {
        // Blink red based on current seconds
        let show_rec = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() / 500) % 2 == 0;
        
        if show_rec {
            transport_spans.push(Span::styled(" ● REC ", Style::default().bg(COLOR_RED).fg(COLOR_BASE).bold()));
        } else {
            transport_spans.push(Span::styled("   REC ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_RED).bold()));
        }
    } else {
        transport_spans.push(Span::styled(" ○ rec ", Style::default().fg(COLOR_SUBTEXT)));
    }

    transport_spans.push(Span::raw("   "));

    // Tempo BPM
    transport_spans.push(Span::styled(format!("♩ BPM: {:.0} ", sequencer.bpm), Style::default().fg(COLOR_PEACH).bold()));

    let transport_widget = Paragraph::new(Line::from(transport_spans)).alignment(Alignment::Right);
    f.render_widget(transport_widget, chunks[2]);
}

fn draw_help_screen(f: &mut Frame, area: Rect) {
    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" HELP GUIDE & DOCUMENTATION ", Style::default().fg(COLOR_MAUVE).bold()));

    f.render_widget(help_block, area);

    let inner = area.inner(&Margin { horizontal: 2, vertical: 2 });
    
    let text = vec![
        Line::from(Span::styled("Welcome to MIDI-TUI DAW: A fully-functional terminal music workstation!", Style::default().fg(COLOR_TEAL).bold())),
        Line::from(""),
        Line::from(Span::styled("Global Navigation Commands:", Style::default().fg(COLOR_MAUVE).bold())),
        Line::from(vec![
            Span::styled("  1 , 2 , 3 , 4 , 5  ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()),
            Span::raw(" - Instantly switch between dashboard workspaces."),
        ]),
        Line::from(vec![
            Span::styled("  Space  ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()),
            Span::raw(" (Global)         - Play / Stop playback sequence."),
        ]),
        Line::from(vec![
            Span::styled("  R  ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()),
            Span::raw(" (Global)             - Armed Recording toggles on/off. Captures live notes."),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl + S / Ctrl + O  ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()),
            Span::raw(" - Save current composition / Load previous project file."),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl + E  ", Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()),
            Span::raw("            - Export track MIDI notes to multi-track Standard MIDI File (.mid)."),
        ]),
        Line::from(""),
        Line::from(Span::styled("Workspace-Specific Hotkeys:", Style::default().fg(COLOR_MAUVE).bold())),
        Line::from(vec![
            Span::styled("  Piano Roll (1):    ", Style::default().fg(COLOR_TEAL).bold()),
            Span::raw("Arrows or Mouse navigate. Space/Left-Click places notes. Right-Click/Space deletes. Shift+Arrows sizes/transposes. [ / ] changes Grid Snap. - / + changes default note duration."),
        ]),
        Line::from(vec![
            Span::styled("  Synth Editor (2):  ", Style::default().fg(COLOR_TEAL).bold()),
            Span::raw("Use Tab/Shift+Tab to select parameters (dials/sliders). Arrow Left/Right to adjust values."),
        ]),
        Line::from(vec![
            Span::styled("  Track Mixer (3):   ", Style::default().fg(COLOR_TEAL).bold()),
            Span::raw("Tab selects track channel. Up/Down adjusts volumes, Left/Right pans. M toggles Mute, S toggles Solo."),
        ]),
        Line::from(vec![
            Span::styled("  Device Manager (4):", Style::default().fg(COLOR_TEAL).bold()),
            Span::raw("Arrow Up/Down selects MIDI port, Enter connects/disconnects controller connection."),
        ]),
        Line::from(""),
        Line::from(Span::styled("Offline Computer QWERTY keyboard virtual synthesizer mapping:", Style::default().fg(COLOR_MAUVE).bold())),
        Line::from("  White Keys: [ A S D F G H J K L ; ]  maps to notes C4 to E5"),
        Line::from("  Black Keys: [ W E   T Y U   O P   ]  maps to sharps/flats"),
    ];

    let p = Paragraph::new(text).alignment(Alignment::Left);
    f.render_widget(p, inner);
}

fn draw_save_modal(f: &mut Frame, input_text: &str) {
    let size = f.size();
    
    // Centered modal box calculations
    let width = 50;
    let height = 7;
    let x = (size.width - width) / 2;
    let y = (size.height - height) / 2;
    let modal_area = Rect { x, y, width, height };

    // Clear background beneath modal
    let clear_widget = Clear;
    f.render_widget(clear_widget, modal_area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_YELLOW).bold())
        .title(Span::styled(" SAVE PROJECT TO DISK ", Style::default().fg(COLOR_YELLOW).bold()));

    let modal_text = vec![
        Line::from("  Specify filename (saves as .json):"),
        Line::from(format!("  ▶  {}█", input_text)),
        Line::from(""),
        Line::from(Span::styled("  [Press ENTER to Save, ESC to Cancel]", Style::default().fg(COLOR_SUBTEXT).italic())),
    ];

    let p = Paragraph::new(modal_text).block(modal_block);
    f.render_widget(p, modal_area);
}

fn draw_load_modal(f: &mut Frame, files: &[String], selected_idx: usize) {
    let size = f.size();
    
    let width = 50;
    let height = 12;
    let x = (size.width - width) / 2;
    let y = (size.height - height) / 2;
    let modal_area = Rect { x, y, width, height };

    // Clear background
    let clear_widget = Clear;
    f.render_widget(clear_widget, modal_area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_TEAL).bold())
        .title(Span::styled(" LOAD SAVED PROJECT ", Style::default().fg(COLOR_TEAL).bold()));

    let mut items = Vec::new();
    if files.is_empty() {
        items.push(ListItem::new("  No saved projects found."));
    } else {
        for (i, file) in files.iter().enumerate() {
            let is_selected = i == selected_idx;
            let style = if is_selected {
                Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()
            } else {
                Style::default().fg(COLOR_TEXT)
            };
            items.push(ListItem::new(format!("  ▶  {}", file)).style(style));
        }
    }

    let list = List::new(items)
        .block(modal_block)
        .highlight_symbol(" ");

    f.render_widget(list, modal_area);
}

fn draw_toast_message(f: &mut Frame, message: &str) {
    let size = f.size();
    
    // Bottom-right toast calculations
    let width = (message.len() + 6) as u16;
    let height = 3;
    let x = size.width.saturating_sub(width).saturating_sub(2);
    let y = size.height.saturating_sub(height).saturating_sub(2);
    let toast_area = Rect { x, y, width, height };

    f.render_widget(Clear, toast_area);

    let toast_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_GREEN).bold());

    let p = Paragraph::new(format!("  {}  ", message))
        .block(toast_block)
        .style(Style::default().fg(COLOR_GREEN).bold())
        .alignment(Alignment::Center);

    f.render_widget(p, toast_area);
}
