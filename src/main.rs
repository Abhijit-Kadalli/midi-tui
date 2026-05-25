mod synth;
mod audio;
mod midi;
mod sequencer;
mod config;
mod ui;
mod app;

use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    event::{self, Event, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use crate::app::App;
use crate::ui::draw_main_ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Set panic hook to ensure terminal raw mode is disabled on panics
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    // Create App state
    let mut app = match App::new() {
        Ok(a) => a,
        Err(e) => {
            // Teardown terminal before printing error
            disable_raw_mode()?;
            execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
            eprintln!("Initialization Error: {:?}", e);
            return Ok(());
        }
    };

    let mut last_tick = Instant::now();
    let mut last_seq_tick = Instant::now();
    let render_interval = Duration::from_millis(16); // ~60 FPS update rate

    loop {
        // Compute active grid layout dimensions for mouse events
        let size = terminal.size()?;
        
        // Vertical layout splits: Header (3), Body (Min 10), Footer (10)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(10),
            ])
            .split(size);

        let body_area = main_chunks[1];
        
        // Horizontal layout splits inside Piano Roll: Keyboard (8), Grid (Min 20)
        let grid_area = Rect {
            x: body_area.x + 8,
            y: body_area.y,
            width: body_area.width.saturating_sub(8),
            height: body_area.height,
        };
        app.grid_rendered_area = grid_area;

        // Render main TUI Frame
        terminal.draw(|f| {
            draw_main_ui(
                f,
                &app.sequencer,
                &app.audio_engine.synth,
                &app.midi_manager,
                &app.audio_engine.visualizer_buf,
                app.active_tab,
                &mut app.piano_roll_state,
                app.focused_synth_field,
                app.focused_mixer_field,
                app.focused_mixer_track,
                app.selected_device_idx,
                &mut app.modal_state,
                &mut app.sparkles,
            );
        })?;

        // 1. Precise sequencer playback playhead calculations
        if app.sequencer.is_playing {
            let seq_tick_duration = app.sequencer.tick_duration();
            let elapsed = last_seq_tick.elapsed();
            
            if elapsed >= seq_tick_duration {
                // Multi-ticking if thread slept too long to prevent latency drag
                let ticks_to_advance = (elapsed.as_micros() / seq_tick_duration.as_micros()) as u32;
                for _ in 0..ticks_to_advance.min(4) { // limit multi-ticks
                    app.sequencer.advance_tick(&app.audio_engine.synth);
                }
                last_seq_tick = Instant::now();
            }
        } else {
            // Keep playback timer sync updated
            last_seq_tick = Instant::now();
        }

        // 2. Process real-time live recording input queues
        app.process_recording_events();
        
        // 3. Process active virtual QWERTY keys hold/release debounce ticks
        app.tick_virtual_keys();

        // 4. Process asynchronous terminal keyboard/mouse inputs
        let timeout = render_interval
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(1));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    // Trigger key presses, if it returns true, request exit
                    if app.trigger_key_press(key) {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse_click(mouse);
                }
                _ => {}
            }
        }

        last_tick = Instant::now();
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
