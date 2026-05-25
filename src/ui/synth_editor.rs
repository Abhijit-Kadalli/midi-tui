use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::synth::{SynthEngine, InstrumentType, Waveform};
use crate::config::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynthFocusField {
    InstrumentType,
    Waveform,
    Attack,
    Decay,
    Sustain,
    Release,
    FmRatio,
    FmIndex,
    FilterCutoff,
    FilterResonance,
    DelayTime,
    DelayFeedback,
}

impl SynthFocusField {
    pub const ALL_FIELDS: [Self; 12] = [
        Self::InstrumentType,
        Self::Waveform,
        Self::Attack,
        Self::Decay,
        Self::Sustain,
        Self::Release,
        Self::FmRatio,
        Self::FmIndex,
        Self::FilterCutoff,
        Self::FilterResonance,
        Self::DelayTime,
        Self::DelayFeedback,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL_FIELDS.iter().position(|&f| f == self).unwrap_or(0);
        Self::ALL_FIELDS[(idx + 1) % Self::ALL_FIELDS.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL_FIELDS.iter().position(|&f| f == self).unwrap_or(0);
        if idx == 0 {
            Self::ALL_FIELDS[Self::ALL_FIELDS.len() - 1]
        } else {
            Self::ALL_FIELDS[idx - 1]
        }
    }
}

pub fn draw_synth_editor(
    f: &mut Frame,
    area: Rect,
    synth: &mut SynthEngine,
    active_track_idx: usize,
    focused_field: SynthFocusField,
) {
    let inst = &synth.instruments[active_track_idx];
    let track_color = TRACK_COLORS[active_track_idx];

    // Main surrounding block
    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(vec![
            Span::styled(" INSTRUMENT EDITOR ", Style::default().fg(COLOR_MAUVE).bold()),
            Span::styled(format!(" Track {} - {} ", active_track_idx + 1, inst.name), Style::default().fg(track_color)),
        ]);

    f.render_widget(main_block, area);

    // Inner area layouts
    let inner_area = area.inner(&Margin { horizontal: 2, vertical: 2 });
    
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Header / Type Selector
            Constraint::Length(8), // ADSR Envelope sliders
            Constraint::Length(6), // FM Parameters
            Constraint::Min(4),    // Global FX (Filters, Delay)
        ])
        .split(inner_area);

    let type_area = main_chunks[0];
    let adsr_area = main_chunks[1];
    let fm_area = main_chunks[2];
    let fx_area = main_chunks[3];

    // 1. Draw Instrument Type Selector
    let inst_focused = focused_field == SynthFocusField::InstrumentType;
    let inst_style = if inst_focused {
        Style::default().fg(COLOR_YELLOW).bold()
    } else {
        Style::default().fg(COLOR_TEXT)
    };

    let type_text = format!(
        " ENGINE TYPE:  {} Subtractive  {} FM Synthesis  {} Plucked String  {} Drum Synth  {} Additive Organ  {} Super-Saw Lead ",
        if inst.inst_type == InstrumentType::Subtractive { "●" } else { "○" },
        if inst.inst_type == InstrumentType::Fm { "●" } else { "○" },
        if inst.inst_type == InstrumentType::Plucked { "●" } else { "○" },
        if inst.inst_type == InstrumentType::Drum { "●" } else { "○" },
        if inst.inst_type == InstrumentType::Additive { "●" } else { "○" },
        if inst.inst_type == InstrumentType::Supersaw { "●" } else { "○" }
    );

    let type_widget = Paragraph::new(type_text)
        .block(Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(COLOR_SURFACE0))
            .title(Span::styled(" Synthesis Model (Use Left/Right to Cycle) ", inst_style))
        );
    f.render_widget(type_widget, type_area);

    // 2. Draw ADSR Envelope section
    let adsr_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Attack
            Constraint::Percentage(25), // Decay
            Constraint::Percentage(25), // Sustain
            Constraint::Percentage(25), // Release
        ])
        .split(adsr_area);

    // Render Attack Slider
    render_slider(
        f,
        adsr_cols[0],
        "ATTACK",
        inst.adsr.attack,
        0.0,
        2.0,
        "s",
        focused_field == SynthFocusField::Attack,
        track_color,
    );

    // Render Decay Slider
    render_slider(
        f,
        adsr_cols[1],
        "DECAY",
        inst.adsr.decay,
        0.0,
        2.0,
        "s",
        focused_field == SynthFocusField::Decay,
        track_color,
    );

    // Render Sustain Slider
    render_slider(
        f,
        adsr_cols[2],
        "SUSTAIN",
        inst.adsr.sustain,
        0.0,
        1.0,
        "%",
        focused_field == SynthFocusField::Sustain,
        track_color,
    );

    // Render Release Slider
    render_slider(
        f,
        adsr_cols[3],
        "RELEASE",
        inst.adsr.release,
        0.0,
        3.0,
        "s",
        focused_field == SynthFocusField::Release,
        track_color,
    );

    // 3. Draw Engine Specifics (Subtractive Waveform OR FM/Additive/Supersaw knobs)
    if inst.inst_type == InstrumentType::Subtractive {
        let wf_focused = focused_field == SynthFocusField::Waveform;
        let wf_style = if wf_focused {
            Style::default().fg(COLOR_YELLOW).bold()
        } else {
            Style::default().fg(COLOR_TEXT)
        };

        let wf_text = format!(
            " WAVEFORM:  {} Sine  {} Square  {} Sawtooth  {} Triangle ",
            if inst.waveform == Waveform::Sine { "●" } else { "○" },
            if inst.waveform == Waveform::Square { "●" } else { "○" },
            if inst.waveform == Waveform::Saw { "●" } else { "○" },
            if inst.waveform == Waveform::Triangle { "●" } else { "○" }
        );

        let wf_widget = Paragraph::new(wf_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(COLOR_SURFACE0))
                .title(Span::styled(" Oscillator Configuration ", wf_style))
            );
        f.render_widget(wf_widget, fm_area);
    } else if inst.inst_type == InstrumentType::Fm || inst.inst_type == InstrumentType::Additive || inst.inst_type == InstrumentType::Supersaw {
        let fm_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(fm_area);

        let (label_ratio, min_ratio, max_ratio, unit_ratio, val_ratio) = match inst.inst_type {
            InstrumentType::Fm => ("FM MOD RATIO", 0.25, 8.0, "x", inst.fm_ratio),
            InstrumentType::Additive => ("HARMONIC SPACING", 0.25, 4.0, "x", inst.fm_ratio),
            InstrumentType::Supersaw => ("DETUNE SPREAD", 0.0, 1.0, "%", inst.fm_ratio),
            _ => ("", 0.0, 1.0, "", 0.0),
        };

        let (label_index, min_index, max_index, unit_index, val_index) = match inst.inst_type {
            InstrumentType::Fm => ("FM MOD INDEX (TIMBRE)", 0.0, 20.0, "", inst.fm_index),
            InstrumentType::Additive => ("HARMONIC DECAY SLOPE", 0.0, 20.0, "", inst.fm_index),
            InstrumentType::Supersaw => ("SUPER-SAW OSC COUNT", 2.0, 6.0, " voices", inst.fm_index),
            _ => ("", 0.0, 1.0, "", 0.0),
        };

        render_slider(
            f,
            fm_cols[0],
            label_ratio,
            val_ratio,
            min_ratio,
            max_ratio,
            unit_ratio,
            focused_field == SynthFocusField::FmRatio,
            COLOR_MAUVE,
        );

        render_slider(
            f,
            fm_cols[1],
            label_index,
            val_index,
            min_index,
            max_index,
            unit_index,
            focused_field == SynthFocusField::FmIndex,
            COLOR_MAUVE,
        );
    } else {
        // Empty placeholder block for Plucked and Drums which have fewer controls
        let msg = match inst.inst_type {
            InstrumentType::Plucked => " Plucked String physical modeling: Tweaking Attack/Decay shapes physical body excitement. ",
            InstrumentType::Drum => " Drum Synth engine: Pitches C2 maps to Kick, D2 to Snare, F#2 to Hi-Hat. ",
            _ => "",
        };
        let placeholder = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(COLOR_SURFACE0))
            );
        f.render_widget(placeholder, fm_area);
    }

    // 4. Draw Global FX (Filter & Delay)
    let fx_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Filter Cutoff
            Constraint::Percentage(25), // Filter Res
            Constraint::Percentage(25), // Delay Time
            Constraint::Percentage(25), // Delay Feedback
        ])
        .split(fx_area);

    render_knob(
        f,
        fx_cols[0],
        "FILTER FREQ",
        synth.filter_cutoff,
        50.0,
        8000.0,
        "Hz",
        focused_field == SynthFocusField::FilterCutoff,
        COLOR_TEAL,
    );

    render_knob(
        f,
        fx_cols[1],
        "FILTER RES",
        synth.filter_resonance,
        0.1,
        5.0,
        "",
        focused_field == SynthFocusField::FilterResonance,
        COLOR_TEAL,
    );

    render_knob(
        f,
        fx_cols[2],
        "DELAY TIME",
        synth.delay_time,
        0.05,
        1.0,
        "s",
        focused_field == SynthFocusField::DelayTime,
        COLOR_BLUE,
    );

    render_knob(
        f,
        fx_cols[3],
        "DELAY FEEDBACK",
        synth.delay_feedback,
        0.0,
        0.95,
        "%",
        focused_field == SynthFocusField::DelayFeedback,
        COLOR_BLUE,
    );
}

// Beautiful slider renderer
fn render_slider(
    f: &mut Frame,
    area: Rect,
    label: &str,
    val: f32,
    min: f32,
    max: f32,
    unit: &str,
    is_focused: bool,
    color: Color,
) {
    let percent = ((val - min) / (max - min)).clamp(0.0, 1.0);
    
    // Draw visual track: 12 steps
    let steps = 12;
    let filled_steps = (percent * steps as f32).round() as usize;
    let mut track_spans = Vec::new();
    for i in 0..steps {
        if i == filled_steps {
            track_spans.push(Span::styled("◯", Style::default().fg(COLOR_YELLOW).bold()));
        } else if i < filled_steps {
            track_spans.push(Span::styled("━", Style::default().fg(color)));
        } else {
            track_spans.push(Span::styled("─", Style::default().fg(COLOR_SURFACE0)));
        }
    }

    let val_display = if unit == "%" {
        format!("{:.0}%", val * 100.0)
    } else {
        format!("{:.2}{}", val, unit)
    };

    let title_style = if is_focused {
        Style::default().fg(COLOR_YELLOW).bold()
    } else {
        Style::default().fg(COLOR_SUBTEXT)
    };

    let body_lines = vec![
        Line::from(""),
        Line::from(track_spans),
        Line::from(vec![Span::raw("  "), Span::styled(val_display, Style::default().fg(COLOR_TEXT))]),
    ];

    let slider_widget = Paragraph::new(body_lines)
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if is_focused { COLOR_YELLOW } else { COLOR_SURFACE0 }))
            .title(Span::styled(format!(" {} ", label), title_style))
        );

    f.render_widget(slider_widget, area);
}

// Gorgeous physical dial knob using Unicode clock rotation symbols!
fn render_knob(
    f: &mut Frame,
    area: Rect,
    label: &str,
    val: f32,
    min: f32,
    max: f32,
    unit: &str,
    is_focused: bool,
    color: Color,
) {
    let percent = ((val - min) / (max - min)).clamp(0.0, 1.0);
    
    // Select clock icon based on rotation percent:
    // 🕐 🕑 🕒 🕓 🕔 🕕 🕖 🕗 🕘 🕙 🕚 🕛
    let clocks = ["◴", "◵", "◶", "◷"];
    let clock_idx = (percent * 3.99) as usize;
    let clock_icon = clocks[clock_idx];

    let val_display = if unit == "%" {
        format!("{:.0}%", val * 100.0)
    } else if val >= 1000.0 {
        format!("{:.1}k{}", val / 1000.0, unit)
    } else {
        format!("{:.2}{}", val, unit)
    };

    let title_style = if is_focused {
        Style::default().fg(COLOR_YELLOW).bold()
    } else {
        Style::default().fg(COLOR_SUBTEXT)
    };

    let dial_str = format!("  ( {} )  ", clock_icon);
    let body_lines = vec![
        Line::from(vec![Span::styled(dial_str, Style::default().fg(color).bold())]),
        Line::from(vec![Span::styled(val_display, Style::default().fg(COLOR_TEXT))]),
    ];

    let knob_widget = Paragraph::new(body_lines)
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if is_focused { COLOR_YELLOW } else { COLOR_SURFACE0 }))
            .title(Span::styled(format!(" {} ", label), title_style))
        );

    f.render_widget(knob_widget, area);
}
