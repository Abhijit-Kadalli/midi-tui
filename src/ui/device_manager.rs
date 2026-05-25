use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::midi::MidiManager;
use crate::config::*;
use cpal::traits::{DeviceTrait, HostTrait};


pub fn draw_device_manager(
    f: &mut Frame,
    area: Rect,
    midi_manager: &MidiManager,
    selected_idx: usize,
) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" DEVICE & CONNECTIONS MANAGER ", Style::default().fg(COLOR_MAUVE).bold()));

    f.render_widget(main_block, area);

    let inner_area = area.inner(&Margin { horizontal: 2, vertical: 2 });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Audio output summary
            Constraint::Min(8),     // MIDI inputs list
        ])
        .split(inner_area);

    let audio_area = chunks[0];
    let midi_area = chunks[1];

    // 1. Draw Audio Device Info
    let host_name = cpal::default_host().id().name();
    let default_output = cpal::default_host()
        .default_output_device()
        .map(|d| d.name().unwrap_or("Unknown".to_string()))
        .unwrap_or("No Audio Outputs Found".to_string());

    let audio_text = vec![
        Line::from(vec![
            Span::raw("System Audio Host:    "),
            Span::styled(host_name, Style::default().fg(COLOR_TEAL).bold()),
        ]),
        Line::from(vec![
            Span::raw("Active Audio Output:  "),
            Span::styled(default_output, Style::default().fg(COLOR_GREEN).bold()),
        ]),
        Line::from(vec![
            Span::raw("Latency Mode:         "),
            Span::styled("Real-Time Low Latency (<15ms)", Style::default().fg(COLOR_BLUE)),
        ]),
        Line::from(vec![
            Span::raw("Output Buffer Size:   "),
            Span::styled("512 Samples @ 44.1kHz / 48.0kHz", Style::default().fg(COLOR_SUBTEXT)),
        ]),
    ];

    let audio_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" System Audio Synthesis ", Style::default().fg(COLOR_TEAL).bold()));

    let audio_p = Paragraph::new(audio_text)
        .block(audio_block)
        .alignment(Alignment::Left);
    
    f.render_widget(audio_p, audio_area);

    // 2. Draw MIDI Port List
    let midi_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SURFACE0))
        .title(Span::styled(" Available Hardware MIDI Input Ports ", Style::default().fg(COLOR_MAUVE).bold()));

    let mut items = Vec::new();
    
    if midi_manager.ports.is_empty() {
        items.push(ListItem::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ✗ No external MIDI controllers detected. ", Style::default().fg(COLOR_RED).italic()),
            ]),
            Line::from(vec![
                Span::styled("    Standard QWERTY keyboard mapping active! Press keys 'A' through ';' to play.", Style::default().fg(COLOR_SUBTEXT)),
            ]),
        ]));
    } else {
        for (i, port_name) in midi_manager.ports.iter().enumerate() {
            let is_selected = i == selected_idx;
            let is_connected = midi_manager.connected_port_name.as_ref()
                .map(|name| name == port_name)
                .unwrap_or(false);

            let mut spans = Vec::new();
            if is_connected {
                spans.push(Span::styled("  [CONNECTED]  ", Style::default().bg(COLOR_GREEN).fg(COLOR_BASE).bold()));
                spans.push(Span::styled(format!(" {}", port_name), Style::default().fg(COLOR_GREEN).bold()));
            } else {
                spans.push(Span::styled("  [AVAILABLE]  ", Style::default().fg(COLOR_SUBTEXT)));
                spans.push(Span::styled(format!(" {}", port_name), Style::default().fg(COLOR_TEXT)));
            }

            let style = if is_selected {
                Style::default().bg(COLOR_SURFACE0).fg(COLOR_YELLOW).bold()
            } else {
                Style::default()
            };

            items.push(ListItem::new(Line::from(spans)).style(style));
        }
    }

    let list = List::new(items)
        .block(midi_block)
        .highlight_symbol(" ▶ ");

    f.render_widget(list, midi_area);

    // Render instructions footer
    if !midi_manager.ports.is_empty() {
        let help_rect = Rect {
            x: midi_area.x + 2,
            y: midi_area.y + midi_area.height - 2,
            width: midi_area.width.saturating_sub(4),
            height: 1,
        };
        let help_text = Paragraph::new("Use UP/DOWN to navigate, ENTER to Connect/Disconnect.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(COLOR_SUBTEXT).italic());
        f.render_widget(help_text, help_rect);
    }
}
