use std::fs;
use tui_input::Input;

pub struct App {
    pub input: Input,
    pub ai_response: String,
    pub is_loading: bool,
    pub pasting: bool,
    pub loading_frames: u64,
    pub scroll: u16,
    pub input_scroll: u16,
}

impl App {
    pub fn new() -> App {
        App {
            input: Input::default(),
            ai_response: String::with_capacity(1024 * 100), // 🚀 Pre-allocate 100KB
            is_loading: false,
            pasting: false,
            loading_frames: 0,
            scroll: 0,
            input_scroll: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.is_loading {
            self.loading_frames = self.loading_frames.wrapping_add(1);
        }
    }

    pub fn auto_scroll(&mut self, height: u16) {
        let lines = self.ai_response.lines().count() as u16;
        if lines > height {
            self.scroll = lines.saturating_sub(height);
        }
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }
    pub fn scroll_down(&mut self, amount: u16) {
        let line_count = self.ai_response.lines().count() as u16;
        if self.scroll + amount < line_count {
            self.scroll = self.scroll.saturating_add(amount);
        }
    }

    pub fn scroll_input_up(&mut self, amount: u16) {
        self.input_scroll = self.input_scroll.saturating_sub(amount);
    }
    pub fn scroll_input_down(&mut self, amount: u16) {
        let line_count = self.input.value().lines().count() as u16;
        if self.input_scroll + amount < line_count {
            self.input_scroll = self.input_scroll.saturating_add(amount);
        }
    }

    // 🚀 THE FIX: Atomic Insertion
    pub fn insert_text(&mut self, text: &str) {
        let current_val = self.input.value();
        let cursor_pos = self.input.cursor();

        // Pre-allocate the full final length to avoid re-allocations
        let mut new_val = String::with_capacity(current_val.len() + text.len());

        let (left, right) = current_val.split_at(cursor_pos);
        new_val.push_str(left);
        new_val.push_str(text);
        new_val.push_str(right);

        let new_cursor = cursor_pos + text.chars().count();
        self.input = Input::new(new_val).with_cursor(new_cursor);
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
                    if !filename.contains("..") {
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
