use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::sequencer::Sequencer;
use crate::config::*;

#[derive(Debug, Clone)]
pub struct PianoRollState {
    pub active_track_idx: usize,
    pub scroll_tick: u32,
    pub scroll_pitch: u8,
    pub cursor_tick: u32,
    pub cursor_pitch: u8,
    pub selected_note_idx: Option<usize>,
    pub note_duration_default: u32, // default ticks for new notes
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            active_track_idx: 0,
            scroll_tick: 0,
            scroll_pitch: 48, // Start vertical view around C3 (note 48)
            cursor_tick: 0,
            cursor_pitch: 60, // Cursor at C4 (note 60)
            selected_note_idx: None,
            note_duration_default: 4, // 1 beat
        }
    }
}

pub fn draw_piano_roll(
    f: &mut Frame,
    area: Rect,
    sequencer: &Sequencer,
    state: &mut PianoRollState,
) {
    let active_track = &sequencer.tracks[state.active_track_idx];
    let track_color = TRACK_COLORS[state.active_track_idx];

    // Define layouts
    // Left: Keyboard roll (7 characters wide)
    // Right: Grid sequencer (the rest)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(20),
        ])
        .split(area);

    let kbd_area = chunks[0];
    let grid_area = chunks[1];

    let grid_height = grid_area.height.saturating_sub(2) as usize; // height without top/bottom borders
    let grid_width = grid_area.width.saturating_sub(2) as usize;

    if grid_height == 0 || grid_width == 0 {
        return;
    }

    // Scroll calculations
    // Keep cursor visible vertically
    let min_pitch = state.scroll_pitch;
    let max_pitch = state.scroll_pitch + grid_height as u8 - 1;
    if state.cursor_pitch < min_pitch {
        state.scroll_pitch = state.cursor_pitch;
    } else if state.cursor_pitch > max_pitch {
        state.scroll_pitch = state.cursor_pitch - grid_height as u8 + 1;
    }

    // Keep cursor visible horizontally
    let min_tick = state.scroll_tick;
    let max_tick = state.scroll_tick + grid_width as u32 - 1;
    if state.cursor_tick < min_tick {
        state.scroll_tick = state.cursor_tick;
    } else if state.cursor_tick > max_tick {
        state.scroll_tick = state.cursor_tick - grid_width as u32 + 1;
    }

    // 1. Draw Keyboard Roll on Left
    draw_keyboard_roll(f, kbd_area, state.scroll_pitch, grid_height, state.cursor_pitch);

    // 2. Draw Sequencer Grid on Right
    // We construct a custom grid using a double buffer or direct character writing in f.buffer_mut()
    let start_x = grid_area.x + 1;
    let start_y = grid_area.y + 1;

    // Draw borders & title
    let grid_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(vec![
            Span::styled(format!(" TRACK {} ({}) ", state.active_track_idx + 1, active_track.name), Style::default().fg(track_color).bold()),
            Span::styled(format!(" Cursor: {} (Tick {}) ", pitch_to_name(state.cursor_pitch), state.cursor_tick), Style::default().fg(COLOR_TEXT)),
            Span::styled(format!(" Snap: {} [Default Len: {} ticks] [Vol: {}%] ", 
                match sequencer.snap_division {
                    1 => "1/16",
                    2 => "1/8",
                    4 => "1/4",
                    8 => "1/2",
                    _ => "1 Bar",
                },
                state.note_duration_default,
                active_track.notes.iter().find(|n| n.pitch == state.cursor_pitch && n.start_tick == state.cursor_tick).map(|n| n.velocity).unwrap_or(100)
            ), Style::default().fg(COLOR_SUBTEXT)),
        ]);
    
    f.render_widget(grid_block, grid_area);

    // Grid rendering logic:
    // Outer loop over Y (pitches, from high to low)
    // Inner loop over X (ticks, from scroll_tick onwards)
    for y_offset in 0..grid_height {
        let draw_y = start_y + y_offset as u16;
        let pitch = state.scroll_pitch + grid_height as u8 - 1 - y_offset as u8;
        let is_black = is_black_key(pitch);

        for x_offset in 0..grid_width {
            let draw_x = start_x + x_offset as u16;
            let tick = state.scroll_tick + x_offset as u32;

            if tick >= sequencer.max_ticks {
                // Beyond song length, draw empty space with dark gray dots
                f.buffer_mut().get_mut(draw_x, draw_y)
                    .set_char('·')
                    .set_style(Style::default().fg(COLOR_SURFACE0));
                continue;
            }

            // Determine baseline background style for the grid cell
            let is_beat = (tick % sequencer.ticks_per_beat) == 0;
            let is_measure = (tick % (sequencer.ticks_per_beat * 4)) == 0;

            let mut cell_char = if is_measure { '┃' } else if is_beat { '│' } else { '░' };
            let mut cell_style = if is_measure {
                Style::default().fg(COLOR_SURFACE0)
            } else if is_beat {
                Style::default().fg(COLOR_SURFACE0)
            } else if is_black {
                Style::default().fg(Color::Rgb(40, 42, 54)) // darker dots for black keys
            } else {
                Style::default().fg(Color::Rgb(55, 59, 75)) // lighter dots for white keys
            };

            // Crosshair cursor highlights
            let is_cursor_row = pitch == state.cursor_pitch;
            let is_cursor_col = tick == state.cursor_tick;

            if is_cursor_row && is_cursor_col {
                cell_style = Style::default().bg(COLOR_SURFACE1).fg(COLOR_YELLOW).bold();
                cell_char = '╬';
            } else if is_cursor_row {
                cell_style = cell_style.bg(COLOR_SURFACE0);
                if cell_char == '░' { cell_char = '─'; }
            } else if is_cursor_col {
                cell_style = cell_style.bg(COLOR_SURFACE0);
                if cell_char == '░' { cell_char = '│'; }
            }

            // Check if there's a note at this cell
            if let Some(note) = sequencer.get_note_at(state.active_track_idx, pitch, tick) {
                // Highlight color depending on if it's the start or body of the note
                let is_start = note.start_tick == tick;
                let is_selected = state.selected_note_idx.map(|idx| {
                    if let Some(sel_n) = active_track.notes.get(idx) {
                        sel_n.pitch == note.pitch && sel_n.start_tick == note.start_tick
                    } else {
                        false
                    }
                }).unwrap_or(false);

                let fill_color = if is_selected {
                    COLOR_YELLOW
                } else {
                    track_color
                };

                cell_char = if is_start { '█' } else { '═' };
                cell_style = Style::default().bg(fill_color).fg(COLOR_BASE).bold();
            }

            // Highlight Playhead position
            if sequencer.is_playing && sequencer.current_tick == tick {
                // If there's already a note, overlay visual highlight
                if sequencer.get_note_at(state.active_track_idx, pitch, tick).is_some() {
                    cell_style = cell_style.bg(COLOR_GREEN);
                } else {
                    cell_style = cell_style.bg(COLOR_SURFACE1).fg(COLOR_GREEN).bold();
                    if cell_char == '░' || cell_char == '─' {
                        cell_char = '█';
                    }
                }
            }

            // Write character to screen buffer
            f.buffer_mut().get_mut(draw_x, draw_y)
                .set_char(cell_char)
                .set_style(cell_style);
        }
    }
}

fn draw_keyboard_roll(
    f: &mut Frame,
    area: Rect,
    scroll_pitch: u8,
    height: usize,
    cursor_pitch: u8,
) {
    let start_x = area.x;
    let start_y = area.y + 1; // within borders

    // Draw frame block
    let kbd_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0));
    
    f.render_widget(kbd_block, area);

    for y_offset in 0..height {
        let draw_y = start_y + y_offset as u16;
        let pitch = scroll_pitch + height as u8 - 1 - y_offset as u8;
        let note_name = pitch_to_name(pitch);
        let is_black = is_black_key(pitch);

        let is_cursor = pitch == cursor_pitch;

        // Label style: black keys have dark background, white keys have light background
        let mut style = if is_black {
            Style::default().bg(COLOR_BASE).fg(COLOR_MAUVE)
        } else {
            Style::default().bg(COLOR_SURFACE0).fg(COLOR_TEXT)
        };

        if is_cursor {
            style = Style::default().bg(COLOR_YELLOW).fg(COLOR_BASE).bold();
        }

        let label = format!("{: ^6}", note_name);
        
        // Write key label
        for (char_idx, c) in label.chars().enumerate() {
            f.buffer_mut().get_mut(start_x + 1 + char_idx as u16, draw_y)
                .set_char(c)
                .set_style(style);
        }
    }
}

// Utility to convert MIDI pitch to string representation
pub fn pitch_to_name(pitch: u8) -> String {
    let oct = (pitch / 12) as i32 - 1;
    let note_idx = pitch % 12;
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", names[note_idx as usize], oct)
}

pub fn is_black_key(pitch: u8) -> bool {
    let note_idx = pitch % 12;
    // Black indices: 1 (C#), 3 (D#), 6 (F#), 8 (G#), 10 (A#)
    matches!(note_idx, 1 | 3 | 6 | 8 | 10)
}
