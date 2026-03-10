use ratatui::{
    layout::{ Constraint, Direction, Layout },
    style::{ Color, Style, Stylize, Modifier },
    text::{ Line, Span },
    widgets::{ Block, Borders, Paragraph, Wrap, BorderType },
    Frame,
};
use crate::app::App;
use ollama_rs::generation::chat::MessageRole;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([
            Constraint::Length(7), // Logo
            Constraint::Length(1), // Status Bar
            Constraint::Min(1), // Conversation
            Constraint::Length(3), // Input
        ])
        .split(f.size());

    // 1. Logo (AI-Mentor© BugMentor)
    let logo_text =
        r#"
 █████╗ ██╗      ███╗   ███╗███████╗███╗   ██╗████████╗ ██████╗ ██████╗       ▄▀▀▄  ▄▀▀▄           ___           __  __         _
██╔══██╗██║      ████╗ ████║██╔════╝████╗  ██║╚══██╔══╝██╔═══██╗██╔══██╗     ▀▄ ██████ ▀          | _ )_  _ __ _|  \/  |___ _ _| |_ ___ _ _ 
███████║██║█████╗██╔████╔██║█████╗  ██╔██╗ ██║   ██║   ██║   ██║██████╔╝   ▄████▌ ██ ▐████▄       | _ \ || / _` | |\/| / -_) ' \  _/ _ \ '_|
██╔══██║██║╚════╝██║╚██╔╝██║██╔══╝  ██║╚██╗██║   ██║   ██║   ██║██╔══██╗   ██▀██▌ ██ ▐██▀██       |___/\_,_\__, |_|  |_\___|_||_\__\___/_|  
██║  ██║██║      ██║ ╚═╝ ██║███████╗██║ ╚████║   ██║   ╚██████╔╝██║  ██║   █████▌ ██ ▐█████                |___/
╚═╝  ╚═╝╚═╝      ╚═╝     ╚═╝╚══════╝╚═╝  ╚═══╝   ╚═╝   ╚═════╝  ╚═╝  ╚═╝ ©  ▀▀██▌    ▐██▀▀ "#;

    f.render_widget(Paragraph::new(logo_text).fg(Color::Rgb(253, 195, 2)), chunks[0]);

    // 2. Status Bar
    let status_style = Style::default()
        .bg(Color::Rgb(66, 133, 244))
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let status_text = if app.is_loading {
        let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner = spinner_chars[((app.loading_frames as usize) / 2) % spinner_chars.len()];
        let elapsed = app.get_elapsed_thinking_time();
        format!("  ⏳ MISTRAL IS THINKING... {}  [{}]  (Esc to Cancel)", spinner, elapsed)
    } else {
        "  ● AI-MENTOR READY  |  F5: Multi  |  F6: Paste  |  F7: Clear  |  Esc: Quit".to_string()
    };
    f.render_widget(Paragraph::new(status_text).style(status_style), chunks[1]);

    // 3. Conversation Area
    let mut conversation_lines = Vec::new();

    for (i, msg) in app.chat_history.iter().enumerate() {
        match msg.role {
            MessageRole::User => {
                let display_content = if msg.content.contains("CONTEXT:") && msg.content.contains("USER:") {
                    msg.content.split("USER:").last().unwrap_or(&msg.content).trim()
                } else {
                    msg.content.trim()
                };
                
                conversation_lines.push(
                    Line::from(
                        vec![
                            Span::styled(
                                " 👤 USER ",
                                Style::default()
                                    .bg(Color::Rgb(253, 195, 2))
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD)
                            ),
                            Span::raw(format!(": {}", display_content))
                        ]
                    )
                );
                conversation_lines.push(Line::from("")); // Spacer
            }
            MessageRole::Assistant => {
                conversation_lines.push(
                    Line::from(
                        vec![
                            Span::styled(
                                " 🤖 MISTRAL ",
                                Style::default()
                                    .bg(Color::Cyan)
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD)
                            ),
                            Span::raw(" ")
                        ]
                    )
                );
                if let Some(rendered) = app.rendered_history.get(i) {
                    conversation_lines.extend(rendered.iter().cloned());
                } else {
                    conversation_lines.extend(render_markdown(&msg.content));
                }
                conversation_lines.push(Line::from("")); // Spacer
            }
            _ => {} // Skip System messages
        }
    }

    // Add current streaming response
    if !app.ai_response.is_empty() {
        conversation_lines.push(
            Line::from(
                vec![
                    Span::styled(
                        " 🤖 MISTRAL ",
                        Style::default()
                            .bg(Color::Cyan)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    ),
                    Span::raw(" ")
                ]
            )
        );
        conversation_lines.extend(app.rendered_ai_response.iter().cloned());
    }

    let body = Paragraph::new(conversation_lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::Rgb(66, 133, 244)))
        );

    f.render_widget(body, chunks[2]);


    // 4. Input Area
    let input_title = if app.pasting {
        Span::styled(
            " MULTILINE MODE (F5 to Unlock) ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        )
    } else {
        Span::raw("")
    };

    let input = Paragraph::new(format!("> {}", app.input.value())).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(input_title)
            .border_style(
                Style::default().fg(if app.pasting { Color::Red } else { Color::Rgb(66, 133, 244) })
            )
    );
    f.render_widget(input, chunks[3]);
}

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for line in text.lines() {
        if line.trim().starts_with("```") {
            in_code_block = !in_code_block;
            let lang = line.trim().trim_start_matches('`').trim();
            let label = if in_code_block {
                format!(" 💻 {} ", if lang.is_empty() {
                    "CODE".to_string()
                } else {
                    lang.to_uppercase()
                })
            } else {
                " ────────────────────────── ".to_string()
            };

            lines.push(
                Line::from(
                    vec![
                        Span::styled(
                            label,
                            Style::default()
                                .fg(Color::Rgb(100, 100, 100))
                                .add_modifier(Modifier::DIM)
                        )
                    ]
                )
            );
            continue;
        }

        if in_code_block {
            let code_style = Style::default()
                .fg(Color::Rgb(171, 178, 191))
                .bg(Color::Rgb(40, 44, 52));
            lines.push(Line::from(vec![Span::styled(format!("  {}", line), code_style)]));
        } else {
            // Handle simple bold text
            let mut spans = Vec::new();

            let parts: Vec<&str> = line.split("**").collect();
            for (i, part) in parts.iter().enumerate() {
                if i % 2 == 1 {
                    spans.push(
                        Span::styled(
                            part.to_string(),
                            Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
                        )
                    );
                } else {
                    spans.push(Span::raw(part.to_string()));
                }
            }

            if spans.is_empty() {
                lines.push(Line::from(line.to_string()));
            } else {
                lines.push(Line::from(spans));
            }
        }
    }
    lines
}
