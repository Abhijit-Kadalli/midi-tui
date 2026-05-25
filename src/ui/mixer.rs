use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::synth::{SynthEngine, InstrumentType};
use crate::config::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixerFocusField {
    TrackVolume,
    TrackPan,
}

pub fn draw_mixer(
    f: &mut Frame,
    area: Rect,
    synth: &mut SynthEngine,
    focused_track_idx: usize,
    focused_field: MixerFocusField,
) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" MULTI-TRACK MIXER ", Style::default().fg(COLOR_MAUVE).bold()));

    f.render_widget(main_block, area);

    let inner_area = area.inner(&Margin { horizontal: 2, vertical: 2 });
    
    // Split area into 4 columns (one for each track)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(inner_area);

    // Calculate dynamic VU peaks for each track based on active voices
    let mut track_peaks = [0.0f32; 4];
    {
        // Synthesizer voices mix
        for voice in &synth.voices {
            if !voice.envelope.is_idle() {
                let track_idx = voice.track_idx;
                if track_idx < 4 {
                    // Accumulate current envelope level
                    track_peaks[track_idx] += voice.envelope.current_value * voice.velocity;
                }
            }
        }
    }

    for i in 0..4 {
        let is_track_focused = i == focused_track_idx;
        let inst = &synth.instruments[i];
        let track_color = TRACK_COLORS[i];

        // Draw track channel strip container
        let border_style = if is_track_focused {
            Style::default().fg(COLOR_YELLOW).bold()
        } else {
            Style::default().fg(COLOR_SURFACE0)
        };

        let strip_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(Span::styled(format!(" Track {} ", i + 1), if is_track_focused { Style::default().fg(COLOR_YELLOW).bold() } else { Style::default().fg(track_color) }));

        let strip_area = cols[i];
        f.render_widget(strip_block, strip_area);

        let strip_inner = strip_area.inner(&Margin { horizontal: 1, vertical: 1 });

        // Lay out controls inside channel strip:
        // - Name & Engine indicator (2 lines)
        // - VU Meter (vertical) next to Volume Slider (horizontal/vertical) (8 lines)
        // - Pan Control (2 lines)
        // - Mute / Solo badges (2 lines)
        let strip_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(6),     // VU + Fader
                Constraint::Length(3),  // Panning fader
                Constraint::Length(2),  // Mute / Solo buttons
            ])
            .split(strip_inner);

        // 1. Header
        let name_p = Paragraph::new(vec![
            Line::from(vec![Span::styled(&inst.name, Style::default().fg(track_color).bold())]),
            Line::from(vec![Span::styled(
                match inst.inst_type {
                    InstrumentType::Subtractive => "Virtual Analog",
                    InstrumentType::Fm => "FM Digital",
                    InstrumentType::Plucked => "Physical Pluck",
                    InstrumentType::Drum => "Drum Synth",
                    InstrumentType::Additive => "Harmonic Organ",
                    InstrumentType::Supersaw => "Super-Saw Lead",
                },
                Style::default().fg(COLOR_SUBTEXT).italic()
            )]),
        ])
        .alignment(Alignment::Center);
        f.render_widget(name_p, strip_chunks[0]);

        // 2. VU & Volume Fader
        let vol_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // VU meter
                Constraint::Percentage(60), // Slider
            ])
            .split(strip_chunks[1]);

        // Draw vertical VU meter
        draw_vu_meter(f, vol_chunks[0], track_peaks[i].min(1.2), inst.mute);

        // Draw vertical volume slider
        let vol_focused = is_track_focused && focused_field == MixerFocusField::TrackVolume;
        draw_vertical_fader(
            f,
            vol_chunks[1],
            inst.volume,
            vol_focused,
            track_color,
        );

        // 3. Pan Control
        let pan_focused = is_track_focused && focused_field == MixerFocusField::TrackPan;
        let pan_label = if inst.pan == 0.0 {
            "Center".to_string()
        } else if inst.pan < 0.0 {
            format!("L {:.0}", inst.pan.abs() * 50.0)
        } else {
            format!("R {:.0}", inst.pan * 50.0)
        };

        let pan_percent = ((inst.pan + 1.0) / 2.0).clamp(0.0, 1.0);
        let pan_steps = 7;
        let pan_filled = (pan_percent * (pan_steps - 1) as f32).round() as usize;
        let mut pan_bar = String::new();
        for p in 0..pan_steps {
            if p == pan_filled {
                pan_bar.push('●');
            } else {
                pan_bar.push('─');
            }
        }

        let pan_style = if pan_focused {
            Style::default().fg(COLOR_YELLOW).bold()
        } else {
            Style::default().fg(COLOR_TEXT)
        };

        let pan_p = Paragraph::new(vec![
            Line::from(vec![Span::styled(format!("Pan: < {} >", pan_bar), pan_style)]),
            Line::from(vec![Span::styled(pan_label, Style::default().fg(COLOR_SUBTEXT))]),
        ])
        .alignment(Alignment::Center)
        .block(Block::default());
        f.render_widget(pan_p, strip_chunks[2]);

        // 4. Mute / Solo badges
        let mut buttons_line = Vec::new();
        if inst.mute {
            buttons_line.push(Span::styled(" MUTE ", Style::default().bg(COLOR_RED).fg(COLOR_BASE).bold()));
        } else {
            buttons_line.push(Span::styled(" mute ", Style::default().fg(COLOR_SUBTEXT)));
        }

        buttons_line.push(Span::raw(" "));

        if inst.solo {
            buttons_line.push(Span::styled(" SOLO ", Style::default().bg(COLOR_GREEN).fg(COLOR_BASE).bold()));
        } else {
            buttons_line.push(Span::styled(" solo ", Style::default().fg(COLOR_SUBTEXT)));
        }

        let btns_p = Paragraph::new(Line::from(buttons_line)).alignment(Alignment::Center);
        f.render_widget(btns_p, strip_chunks[3]);
    }
}

fn draw_vu_meter(f: &mut Frame, area: Rect, peak: f32, is_muted: bool) {
    let height = area.height as usize;
    let width = area.width as usize;

    if height == 0 || width == 0 {
        return;
    }

    // Map peak level [0.0, 1.0] to vertical height steps
    let filled_height = if is_muted {
        0
    } else {
        ((peak * height as f32).round() as usize).min(height)
    };

    let start_x = area.x;
    let start_y = area.y;

    for y in 0..height {
        let draw_y = start_y + y as u16;
        let is_filled = (height - 1 - y) < filled_height;
        
        let color = if is_filled {
            // Colors: top 20% red, next 20% yellow, bottom 60% green
            let percent_height = (height - 1 - y) as f32 / height as f32;
            if percent_height > 0.8 {
                COLOR_RED
            } else if percent_height > 0.6 {
                COLOR_YELLOW
            } else {
                COLOR_GREEN
            }
        } else {
            COLOR_SURFACE0
        };

        for x in 0..width.min(2) {
            f.buffer_mut().get_mut(start_x + x as u16, draw_y)
                .set_char(if is_filled { '█' } else { '░' })
                .set_style(Style::default().fg(color));
        }
    }
}

fn draw_vertical_fader(
    f: &mut Frame,
    area: Rect,
    vol: f32,
    is_focused: bool,
    color: Color,
) {
    let height = area.height as usize;
    let width = area.width as usize;

    if height == 0 || width == 0 {
        return;
    }

    let fader_height = height.saturating_sub(2);
    if fader_height == 0 {
        return;
    }

    let cap_pos = ((1.0 - vol) * (fader_height - 1) as f32).round() as usize;

    let start_x = area.x + (area.width / 2);
    let start_y = area.y + 1;

    let stem_style = Style::default().fg(COLOR_SURFACE0);
    let cap_style = if is_focused {
        Style::default().bg(COLOR_YELLOW).fg(COLOR_BASE).bold()
    } else {
        Style::default().bg(color).fg(COLOR_BASE).bold()
    };

    // Draw vertical tracks
    for y in 0..fader_height {
        let draw_y = start_y + y as u16;
        if y == cap_pos {
            // Fader cap knob
            f.buffer_mut().get_mut(start_x - 1, draw_y).set_char('█').set_style(cap_style);
            f.buffer_mut().get_mut(start_x, draw_y).set_char('█').set_style(cap_style);
            f.buffer_mut().get_mut(start_x + 1, draw_y).set_char('█').set_style(cap_style);
        } else {
            f.buffer_mut().get_mut(start_x, draw_y).set_char('│').set_style(stem_style);
        }
    }

    // Volume label at the bottom
    let db_val = if vol <= 0.0 {
        "-INF".to_string()
    } else {
        format!("{:.1}dB", 20.0 * vol.log10())
    };

    let label_style = if is_focused {
        Style::default().fg(COLOR_YELLOW).bold()
    } else {
        Style::default().fg(COLOR_TEXT)
    };

    let label_p = Paragraph::new(db_val)
        .alignment(Alignment::Center)
        .style(label_style);

    f.render_widget(label_p, Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    });
}
