use ai_mentor_cli::app::App;
use ai_mentor_cli::mentor::{ ask_mentor_chat, MentorEvent };
use ai_mentor_cli::agent::orchestrator::Orchestrator;
use ai_mentor_cli::ui;
use crossterm::{
    event::{ self, Event, KeyCode, EnableBracketedPaste, DisableBracketedPaste, KeyEventKind },
    execute,
    terminal::{ disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen },
};
use ratatui::{ backend::CrosstermBackend, Terminal };
use std::{ error::Error, io, time::Duration };
use tokio::sync::mpsc;
use tui_input::backend::crossterm::EventHandler;
use ollama_rs::generation::chat::{ChatMessage, MessageRole};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let (tx, mut rx) = mpsc::channel(100);
    let (input_tx, mut input_rx) = mpsc::channel(100);

    // ⌨️ Dedicated Input Handler Thread
    let _input_thread = {
        let input_tx = input_tx.clone();
        tokio::task::spawn_blocking(move || {
            loop {
                if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                    if let Ok(ev) = event::read() {
                        if input_tx.blocking_send(ev).is_err() { break; }
                    }
                }
            }
        })
    };

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(30);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        tokio::select! {
            // ⌨️ Handle Input from Channel
            Some(any_event) = input_rx.recv() => {
                if let Event::Key(key) = any_event {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc => {
                                if app.is_loading {
                                    if let Some(task) = app.current_task.take() {
                                        task.abort();
                                        app.is_loading = false;
                                        app.thinking_start_time = None;
                                        let aborted_msg = "Generation aborted by user".to_string();
                                        app.chat_history.push(ChatMessage::new(MessageRole::Assistant, aborted_msg));
                                        app.sync_history(app.chat_history.clone());
                                        app.clear_ai_response();
                                        app.current_task_id += 1;
                                    }
                                } else {
                                    break;
                                }
                            }
                            KeyCode::F(5) => app.pasting = !app.pasting,
                            KeyCode::F(6) => {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    if let Ok(text) = cb.get_text() {
                                        app.insert_text(&text.replace("\r\n", "\n").replace('\r', "\n"));
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
                                 } else if !app.input.value().trim().is_empty() && !app.is_loading {
                                     let prompt = app.input.value().to_string();
                                     
                                     // Save to prompt history
                                     if app.prompt_history.last() != Some(&prompt) {
                                         app.prompt_history.push(prompt.clone());
                                     }
                                     app.history_index = None;

                                     app.is_loading = true;
                                     app.thinking_start_time = Some(std::time::Instant::now());
                                     app.reset_scroll();
                                     app.input = tui_input::Input::default();
                                     app.clear_ai_response(); 
                                     
                                     let tx_c = tx.clone();
                                     let mut history = app.chat_history.clone();
                                     let prompt_async = prompt.clone();
                                     
                                     app.current_task_id += 1;
                                     let task_id = app.current_task_id;

                                     let handle = tokio::spawn(async move {
                                         // Decide: Simple Chat or Full Agent?
                                         let is_complex = prompt_async.len() > 10 || prompt_async.to_lowercase().contains("plan") || prompt_async.to_lowercase().contains("task") || prompt_async.to_lowercase().contains("list") || prompt_async.to_lowercase().contains("read");
                                         
                                         if is_complex {
                                             if let Ok(orchestrator) = Orchestrator::new(tx_c.clone()).await {
                                                 let os = if cfg!(target_os = "windows") { "Windows".to_string() } else { "Linux/macOS".to_string() };
                                                 let shell = if cfg!(target_os = "windows") { "PowerShell".to_string() } else { "sh".to_string() };
                                                 match orchestrator.run_task(task_id, prompt_async.clone(), os, shell).await {
                                                     Ok(_) => {}
                                                     Err(e) => {
                                                         let _ = tx_c.send(MentorEvent::Error(task_id, format!("Task execution failed: {}", e))).await;
                                                     }
                                                 }
                                                 return;
                                             }
                                         }

                                         // Fallback / Simple Mode
                                         let _ = tx_c.send(MentorEvent::Chunk(task_id, "[System: Gathering project context...]\n".to_string())).await;
                                         let context = ai_mentor_cli::rag::get_context(&prompt_async).await;
                                         history.push(ChatMessage::new(MessageRole::User, prompt_async));
                                         let _ = tx_c.send(MentorEvent::Chunk(task_id, "[System: Contacting Mistral...]\n".to_string())).await;
                                         ask_mentor_chat(task_id, history, Some(context), tx_c).await;
                                     });
                                     app.current_task = Some(handle);
                                     app.chat_history.push(ChatMessage::new(MessageRole::User, prompt.clone()));
                                     app.sync_history(app.chat_history.clone());
                                 }
                             }
                             KeyCode::Up => {
                                 if !app.pasting {
                                     let new_idx = match app.history_index {
                                         Some(idx) if idx > 0 => Some(idx - 1),
                                         Some(0) => Some(0),
                                         None if !app.prompt_history.is_empty() => Some(app.prompt_history.len() - 1),
                                         _ => None,
                                     };
                                     if let Some(idx) = new_idx {
                                         app.history_index = Some(idx);
                                         let val = app.prompt_history[idx].clone();
                                         let cursor = val.len();
                                         app.input = tui_input::Input::new(val).with_cursor(cursor);
                                     }
                                 }
                             }
                             KeyCode::Down => {
                                 if !app.pasting {
                                     if let Some(idx) = app.history_index {
                                         if idx + 1 < app.prompt_history.len() {
                                             let next_idx = idx + 1;
                                             app.history_index = Some(next_idx);
                                             let val = app.prompt_history[next_idx].clone();
                                             let cursor = val.len();
                                             app.input = tui_input::Input::new(val).with_cursor(cursor);
                                         } else {
                                             app.history_index = None;
                                             app.input = tui_input::Input::default();
                                         }
                                     }
                                 }
                             }
                             KeyCode::PageUp => {
                                 app.scrolled_manually = true;
                                 app.scroll = app.scroll.saturating_sub(5);
                             }
                             KeyCode::PageDown => {
                                 app.scrolled_manually = true;
                                 app.scroll = app.scroll.saturating_add(5);
                             }

                            _ => { app.input.handle_event(&Event::Key(key)); }
                        }
                    }
                }
            }
            // 🤖 Handle AI Chunks IMMEDIATELY
            Some(ev) = rx.recv() => {
                match ev {
                    MentorEvent::Chunk(id, c) => {
                        if app.is_loading && id == app.current_task_id {
                            app.update_ai_response(&c);
                            let size = terminal.size().unwrap_or_default();
                            app.auto_scroll(size.height.saturating_sub(20));
                        }
                    }
                    MentorEvent::Finished(id) => {
                        if app.is_loading && id == app.current_task_id {
                            let elapsed = app.get_elapsed_thinking_time();
                            app.update_ai_response(&format!("\n\n[Done in {}]", elapsed));
                            app.is_loading = false;
                            app.thinking_start_time = None;
                        }
                    }
                    MentorEvent::HistoryUpdate(_id, new_history) => {
                        if _id == app.current_task_id { app.sync_history(new_history); }
                    }
                    MentorEvent::Error(_id, e) => {
                        if app.is_loading && _id == app.current_task_id {
                            let elapsed = app.get_elapsed_thinking_time();
                            app.update_ai_response(&format!("\n\nErr: {} [Failed after {}]", e, elapsed));
                            app.is_loading = false;
                            app.thinking_start_time = None;
                        }
                    }
                }
            }
            // 🕒 Animations/Ticks
            _ = tokio::time::sleep(Duration::from_millis(16)) => {}
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
