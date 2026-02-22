use std::fs;
use tui_input::Input;

pub struct App {
    pub input: Input,
    pub ai_response: String,
    pub is_loading: bool,
    pub pasting: bool,
}

impl App {
    pub fn new() -> App {
        App {
            input: Input::default(),
            ai_response: String::new(),
            is_loading: false,
            pasting: false,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        let idx = self.input.cursor();
        let current = self.input.value();
        let (left, right): (String, String) = current
            .chars()
            .enumerate()
            .fold((String::new(), String::new()), |(mut l, mut r), (i, c)| {
                if i < idx {
                    l.push(c);
                } else {
                    r.push(c);
                }
                (l, r)
            });
        self.input = Input::new(format!("{}{}{}", left, text, right)).with_cursor(
            idx + text.chars().count()
        );
    }

    pub fn process_agent_actions(&mut self) {
        let ai_text = self.ai_response.clone();
        let mut search_text = ai_text.as_str();
        while let Some(start_idx) = search_text.find("<file name=\"") {
            let after_start = &search_text[start_idx + 12..];
            if let Some(quote_idx) = after_start.find("\">") {
                let filename = &after_start[..quote_idx];
                let after_filename = &after_start[quote_idx + 2..];
                if let Some(end_idx) = after_filename.find("</file>") {
                    if !filename.starts_with("src/") && filename != "Cargo.toml" {
                        let _ = fs::write(filename, after_filename[..end_idx].trim());
                    }
                    search_text = &after_filename[end_idx + 7..];
                    continue;
                }
            }
            break;
        }
    }
}
