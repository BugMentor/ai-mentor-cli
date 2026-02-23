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
        .constraints([Constraint::Length(8), Constraint::Min(1), Constraint::Length(10)])
        .split(f.size());

    let logo =
        r#"
 █████╗ ██╗      ███╗   ███╗███████╗███╗   ██╗████████╗ ██████╗ ██████╗       ▄▀▀▄      ▄▀▀▄       ___           __  __         _           
██╔══██╗██║      ████╗ ████║██╔════╝████╗  ██║╚══██╔══╝██╔═══██╗██╔══██╗     ▀▄ ██████ ▄▀         | _ )_  _ __ _|  \/  |___ _ _| |_ ___ _ _ 
███████║██║█████╗██╔████╔██║█████╗  ██╔██╗ ██║   ██║   ██║   ██║██████╔╝   ▄████▌ ██ ▐████▄       | _ \ || / _` | |\/| / -_) ' \  _/ _ \ '_|
██╔══██║██║╚════╝██║╚██╔╝██║██╔══╝  ██║╚██╗██║   ██║   ██║   ██║██╔══██╗   ██▀██▌ ██ ▐██▀██       |___/\_,_\__, |_|  |_\___|_||_\__\___/_|  
██║  ██║██║      ██║ ╚═╝ ██║███████╗██║ ╚████║   ██║   ╚██████╔╝██║  ██║   █████▌ ██ ▐█████                |___/                            
╚═╝  ╚═╝╚═╝      ╚═╝     ╚═╝╚══════╝╚═╝  ╚═══╝   ╚═╝   ╚═════╝ ╚═╝  ╚═╝ ©    ▀▀██▌    ▐██▀▀                                                 
    "#;

    f.render_widget(Paragraph::new(logo).fg(Color::Rgb(253, 195, 2)), chunks[0]);

    let title = if app.is_loading {
        let dots_count = ((app.loading_frames as usize) / 3) % 4;
        let dots = ".".repeat(dots_count);
        let spaces = " ".repeat(3 - dots_count);
        let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner = spinner_chars[((app.loading_frames as usize) / 2) % spinner_chars.len()];
        format!(" [ ⏳ MISTRAL IS THINKING{}{} {} ] ", dots, spaces, spinner).cyan().bold()
    } else {
        " [ MENTOR READY ] ".green()
    };

    let body = Paragraph::new(app.ai_response.clone())
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(body, chunks[1]);

    let input_title = if app.pasting {
        " 🔒 MULTILINE MODE (F5 to Unlock) | F6 to OS Paste | F7 to Clear ".red()
    } else {
        " [F5: Multiline] [F6: Paste] [F7: Clear] [↑/↓ Scroll In] [Enter: Send] ".yellow()
    };

    let input = Paragraph::new(app.input.value())
        .wrap(Wrap { trim: false })
        .scroll((app.input_scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(input_title)
                .border_style(
                    Style::default().fg(if app.pasting { Color::Red } else { Color::Yellow })
                )
        );
    f.render_widget(input, chunks[2]);
}
