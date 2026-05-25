use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::{Arc, Mutex};
use crate::synth::SynthEngine;
use crate::audio::VisualizerBuffer;
use crate::config::*;
use rand::Rng;

pub fn draw_waveform_visualizer(
    f: &mut Frame,
    area: Rect,
    vis_buf: &Arc<Mutex<VisualizerBuffer>>,
) {
    let samples = {
        let buf = vis_buf.lock().unwrap();
        buf.get_samples()
    };

    let width = area.width as usize;
    let height = (area.height as usize).saturating_sub(2);
    if width == 0 || height == 0 {
        return;
    }

    // Downsample the 512 samples into `width` buckets, preserving peak signs
    let chunk_size = (samples.len() / width).max(1);
    let mut display_vals = vec![0.0f32; width];
    
    for i in 0..width {
        let start = i * chunk_size;
        if start >= samples.len() {
            break;
        }
        let end = (start + chunk_size).min(samples.len());
        let mut peak = 0.0f32;
        for j in start..end {
            let val = samples[j];
            if val.abs() > peak.abs() {
                peak = val;
            }
        }
        display_vals[i] = peak;
    }

    // Create a character grid of size [height][width]
    let mut grid = vec![vec![' '; width]; height];
    let mid = height / 2;

    // Draw baseline
    for x in 0..width {
        grid[mid][x] = '⠤';
    }

    // Draw waveform bars
    for x in 0..width {
        // Apply log-like scaling to scale the visual dynamics while preserving the sign
        let val = display_vals[x];
        let sign = if val >= 0.0 { 1.0 } else { -1.0 };
        let scaled_abs = val.abs().sqrt().clamp(0.0, 1.0);
        let scaled_val = sign * scaled_abs;

        let offset = (scaled_val * (mid as f32)).round() as isize;
        let target_y = (mid as isize - offset).clamp(0, height as isize - 1) as usize;

        if target_y <= mid {
            for y in target_y..=mid {
                // Positive peak elements (fill up)
                grid[y][x] = if y == target_y && y < mid { '▄' } else { '█' };
            }
        } else {
            for y in mid..=target_y {
                // Negative peak elements (fill down)
                grid[y][x] = if y == target_y && y > mid { '▀' } else { '█' };
            }
        }
    }

    // Convert the grid of characters to Paragraph Lines
    let mut lines = Vec::with_capacity(height);
    for y in 0..height {
        let row_str: String = grid[y].iter().collect();
        lines.push(Line::from(vec![
            Span::styled(row_str, Style::default().fg(COLOR_TEAL)),
        ]));
    }

    let wave_widget = Paragraph::new(lines)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_SURFACE0))
            .title(Span::styled(" Real-Time Waveform Analyzer ", Style::default().fg(COLOR_TEAL).bold()))
        );

    f.render_widget(wave_widget, area);
}

// Spark/particle structure for physical keyboard note events
#[derive(Clone)]
pub struct Sparkle {
    pub x: u16,
    pub y: f32, // y rises up
    pub char_glyph: char,
    pub lifetime: f32, // fades out
}

pub fn draw_keyboard_visualizer(
    f: &mut Frame,
    area: Rect,
    synth: &Arc<Mutex<SynthEngine>>,
    sparkles: &mut Vec<Sparkle>,
) {
    let synth_locked = synth.lock().unwrap();

    // Map midi note range C3 (48) to B5 (83) - 3 full octaves (36 keys)
    // White keys count: 3 octaves * 7 = 21 white keys.
    // White keys notes:
    // Octave 3: 48 (C3), 50 (D3), 52 (E3), 53 (F3), 55 (G3), 57 (A3), 59 (B3)
    // Octave 4: 60 (C4), 62 (D4), 64 (E4), 65 (F4), 67 (G4), 69 (A4), 71 (B4)
    // Octave 5: 72 (C5), 74 (D5), 76 (E5), 77 (F5), 79 (G5), 81 (A5), 83 (B5)
    
    let white_keys = [
        48, 50, 52, 53, 55, 57, 59,
        60, 62, 64, 65, 67, 69, 71,
        72, 74, 76, 77, 79, 81, 83,
    ];

    let black_keys = [
        (49, 1),   // C#3
        (51, 2),   // D#3
        (54, 4),   // F#3
        (56, 5),   // G#3
        (58, 6),   // A#3
        (61, 8),   // C#4
        (63, 9),   // D#4
        (66, 11),  // F#4
        (68, 12),  // G#4
        (70, 13),  // A#4
        (73, 15),  // C#5
        (75, 16),  // D#5
        (78, 18),  // F#5
        (80, 19),  // G#5
        (82, 20),  // A#5
    ];

    let start_x = area.x;
    let start_y = area.y;
    let width = area.width;
    let height = area.height.max(4); // Need at least 4 lines for keyboard

    // Calculate layout: 21 white keys.
    // Each white key is `key_width` characters wide
    let key_width = (width / 21).max(2).min(5);
    let total_kbd_width = key_width * 21;
    let offset_x = start_x + (width - total_kbd_width) / 2;

    // Tick sparkles lifetime and y position
    let mut rng = rand::thread_rng();
    for sparkle in sparkles.iter_mut() {
        sparkle.y -= 0.15; // float upwards
        sparkle.lifetime -= 0.05;
        // Float horizontal drift slightly
        if rng.gen_bool(0.3) {
            if rng.gen_bool(0.5) {
                sparkle.x = sparkle.x.saturating_add(1);
            } else {
                sparkle.x = sparkle.x.saturating_sub(1);
            }
        }
    }
    sparkles.retain(|s| s.lifetime > 0.0 && s.y >= start_y as f32);

    // Trigger new sparkles from currently active synth voices
    for voice in &synth_locked.voices {
        if !voice.envelope.is_idle() && voice.note >= 48 && voice.note <= 83 {
            // Find key's visual position to emit sparkles
            let note = voice.note;
            let mut key_x = offset_x;
            
            if let Some(pos) = white_keys.iter().position(|&k| k == note) {
                key_x += pos as u16 * key_width + key_width / 2;
            } else if let Some(idx) = black_keys.iter().position(|&(k, _)| k == note) {
                let (_, w_pos) = black_keys[idx];
                key_x += w_pos as u16 * key_width;
            }

            if rng.gen_bool(0.12) { // limit spawn rate
                let glyphs = ['*', '+', 'o', '°', '✧', '♦'];
                let glyph = glyphs[rng.gen_range(0..glyphs.len())];
                sparkles.push(Sparkle {
                    x: key_x,
                    y: (start_y + height - 2) as f32,
                    char_glyph: glyph,
                    lifetime: 1.0,
                });
            }
        }
    }

    // 1. Render White Keys (Base layers)
    for y_offset in 0..height - 1 {
        let draw_y = start_y + y_offset;
        for (i, &note) in white_keys.iter().enumerate() {
            let key_x_start = offset_x + i as u16 * key_width;
            
            // Check if active
            let is_active = synth_locked.voices.iter().any(|v| v.note == note && !v.envelope.is_idle());
            
            let style = if is_active {
                Style::default().bg(COLOR_GREEN).fg(COLOR_BASE)
            } else {
                Style::default().bg(Color::Rgb(240, 240, 240)).fg(Color::Rgb(80, 80, 80))
            };

            for x_offset in 0..key_width - 1 {
                let char_to_draw = if y_offset == height - 2 && x_offset == key_width / 2 {
                    // Draw a label for the starting notes of octaves
                    if note == 48 { '3' }
                    else if note == 60 { '4' }
                    else if note == 72 { '5' }
                    else { ' ' }
                } else {
                    ' '
                };

                f.buffer_mut().get_mut(key_x_start + x_offset, draw_y)
                    .set_char(char_to_draw)
                    .set_style(style);
            }

            // Draw dark separator line between white keys
            for y_sep in 0..height - 1 {
                f.buffer_mut().get_mut(key_x_start + key_width - 1, start_y + y_sep)
                    .set_char('│')
                    .set_style(Style::default().bg(COLOR_BASE).fg(COLOR_SURFACE0));
            }
        }
    }

    // 2. Render Black Keys (Overlay on top of white keys, occupying top half)
    let black_key_height = (height - 1) * 2 / 3; // Occupy top 66%
    for y_offset in 0..black_key_height {
        let draw_y = start_y + y_offset;
        for &(note, w_idx) in &black_keys {
            let key_x_center = offset_x + w_idx as u16 * key_width;
            
            let is_active = synth_locked.voices.iter().any(|v| v.note == note && !v.envelope.is_idle());

            let style = if is_active {
                Style::default().bg(COLOR_MAUVE).fg(COLOR_BASE)
            } else {
                Style::default().bg(COLOR_SURFACE0).fg(COLOR_TEXT)
            };

            // Black key is 1 or 2 characters wide depending on white key width
            let bk_width = (key_width / 2).max(1);
            let start_bk = key_x_center - bk_width / 2;

            for x_offset in 0..bk_width {
                f.buffer_mut().get_mut(start_bk + x_offset, draw_y)
                    .set_char('█')
                    .set_style(style);
            }
        }
    }

    // 3. Render Floating Sparkles/Particles
    for sparkle in sparkles {
        let s_x = sparkle.x;
        let s_y = sparkle.y.round() as u16;
        
        if s_x >= start_x && s_x < start_x + width && s_y >= start_y && s_y < start_y + height - 1 {
            let mut style = Style::default().fg(COLOR_YELLOW).bold();
            // Fade particles out based on lifetime
            if sparkle.lifetime < 0.3 {
                style = style.fg(COLOR_SUBTEXT);
            } else if sparkle.lifetime < 0.6 {
                style = style.fg(COLOR_TEAL);
            }
            
            f.buffer_mut().get_mut(s_x, s_y)
                .set_char(sparkle.char_glyph)
                .set_style(style);
        }
    }

    // Render title block
    let kbd_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" Polyphonic Keyboard Visualizer ", Style::default().fg(COLOR_MAUVE).bold()));
    
    f.render_widget(kbd_block, area);
}
