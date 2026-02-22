mod app;
mod ui;
mod mentor;
mod rag;
use crate::app::App;
use crate::mentor::{ ask_mentor, MentorEvent };
use crossterm::{
    event::{ self, Event, KeyCode, EnableBracketedPaste, DisableBracketedPaste },
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

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;
        tokio::select! {
            msg = rx.recv() => if let Some(ev) = msg { match ev {
                MentorEvent::Chunk(c) => app.ai_response.push_str(&c),
                MentorEvent::Finished => { app.process_agent_actions(); app.is_loading = false; }
                MentorEvent::Error(e) => { app.ai_response.push_str(&format!("\nError: {}", e)); app.is_loading = false; }
            }},
            res = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(50))) => {
                if let Ok(Ok(true)) = res {
                    if let Event::Key(key) = event::read()? {
                        match key.code {
                            KeyCode::Esc => break,
                            KeyCode::F(5) => app.pasting = !app.pasting,
                            KeyCode::Backspace => { app.input.handle_event(&Event::Key(key)); }
                            KeyCode::Enter => if app.pasting { app.insert_text("\n"); } else {
                                app.is_loading = true;
                                let prompt = app.input.value().to_string();
                                app.input = tui_input::Input::default();
                                app.ai_response.clear();
                                let tx_c = tx.clone();
                                tokio::spawn(async move { ask_mentor(prompt, tx_c).await; });
                            },
                            _ => { app.input.handle_event(&Event::Key(key)); }
                        }
                    }
                }
            }
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableBracketedPaste)?;
    Ok(())
}
