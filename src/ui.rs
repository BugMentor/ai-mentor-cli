use ratatui::{
    layout::{ Constraint, Direction, Layout },
    style::{ Color, Style, Stylize },
    widgets::{ Block, Borders, Paragraph, Wrap },
    Frame,
};
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(7), Constraint::Min(1), Constraint::Length(10)])
        .split(f.size());

    let logo =
        r#"
     █████╗ ██╗      ███╗   ███╗███████╗███╗   ██╗████████╗ ██████╗ ██████╗ 
    ██╔══██╗██║      ████╗ ████║██╔════╝████╗  ██║╚══██╔══╝██╔═══██╗██╔══██╗
    ███████║██║█████╗██╔████╔██║█████╗  ██╔██╗ ██║   ██║   ██║   ██║██████╔╝
    ██╔══██║██║╚════╝██║╚██╔╝██║██╔══╝  ██║╚██╗██║   ██║   ██║   ██║██╔══██╗
    ██║  ██║██║      ██║ ╚═╝ ██║███████╗██║ ╚████║   ██║   ╚██████╔╝██║  ██║
    "#;
    f.render_widget(Paragraph::new(logo).fg(Color::Rgb(253, 195, 2)), chunks[0]);

    let title = if app.is_loading {
        " [ ⏳ MISTRAL IS LOADING/THINKING... ] ".cyan().bold()
    } else {
        " [ MENTOR READY ] ".green()
    };

    let body = Paragraph::new(app.ai_response.as_str())
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(body, chunks[1]);

    let input_title = if app.pasting {
        " 🔒 PASTE MODE (F5 to Unlock) ".red()
    } else {
        " [Input] ".yellow()
    };
    let input = Paragraph::new(app.input.value())
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(input_title));
    f.render_widget(input, chunks[2]);
}
