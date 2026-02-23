mod app;
mod ui;
mod mentor;
mod rag;
use crate::app::App;
use crate::mentor::{ ask_mentor, MentorEvent };
use crossterm::{
    event::{ self, Event, KeyCode, EnableBracketedPaste, DisableBracketedPaste, KeyEventKind },
    execute,
    terminal::{ disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen },
};
use ratatui::{ backend::CrosstermBackend, Terminal };
use std::{ error::Error, io, time::Duration };
use tokio::sync::mpsc;
use tui_input::backend::crossterm::EventHandler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let (tx, mut rx) = mpsc::channel(100);

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(16); // ~60fps for animation

    loop {
        // 🚀 ONLY DRAW IF WE NEED TO
        terminal.draw(|f| ui::draw(f, &app))?;

        // ⚡ BLOCKING POLL WITH TIMEOUT: This is the secret sauce for typing speed.
        // It waits for a key for up to 16ms. If no key arrives, it finishes so the animation can tick.
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => {
                            break;
                        }
                        KeyCode::F(5) => {
                            app.pasting = !app.pasting;
                        }
                        KeyCode::Up => app.scroll_input_up(1),
                        KeyCode::Down => app.scroll_input_down(1),
                        KeyCode::PageUp => app.scroll_up(1),
                        KeyCode::PageDown => app.scroll_down(1),
                        KeyCode::F(6) => {
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                if let Ok(text) = cb.get_text() {
                                    app.insert_text(
                                        &text.replace("\r\n", "\n").replace('\r', "\n")
                                    );
                                }
                            }
                        }
                        KeyCode::F(7) => {
                            app.input = tui_input::Input::default();
                            app.input_scroll = 0;
                        }
                        KeyCode::Enter => {
                            if app.pasting {
                                app.insert_text("\n");
                            } else if !app.input.value().trim().is_empty() {
                                app.is_loading = true;
                                app.scroll = 0;
                                let prompt = app.input.value().to_string();
                                app.input = tui_input::Input::default();
                                app.ai_response.clear();
                                let tx_c = tx.clone();
                                tokio::spawn(async move {
                                    let context = crate::rag::get_context(&prompt).await;
                                    ask_mentor(
                                        format!("CONTEXT:\n{}\n\nUSER: {}", context, prompt),
                                        tx_c
                                    ).await;
                                });
                            }
                        }
                        // 🚀 Typing is now handled by the OS event loop immediately
                        _ => {
                            app.input.handle_event(&Event::Key(key));
                        }
                    }
                }
            }
        }

        // Handle AI chunks
        while let Ok(ev) = rx.try_recv() {
            match ev {
                MentorEvent::Chunk(c) => {
                    app.ai_response.push_str(&c);
                    let size = terminal.size().unwrap_or_default();
                    app.auto_scroll(size.height.saturating_sub(20));
                }
                MentorEvent::Finished => {
                    app.process_agent_actions();
                    app.is_loading = false;
                }
                MentorEvent::Error(e) => {
                    app.ai_response.push_str(&format!("\nErr: {}", e));
                    app.is_loading = false;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = std::time::Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableBracketedPaste)?;
    Ok(())
}
