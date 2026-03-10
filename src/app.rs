use tui_input::Input;
use ollama_rs::generation::chat::{ChatMessage, MessageRole};
use std::time::Instant;
use ratatui::text::Line;

pub struct App {
    pub input: Input,
    pub ai_response: String,
    pub rendered_ai_response: Vec<Line<'static>>,
    pub ai_response_lines: usize,
    pub is_loading: bool,
    pub pasting: bool,
    pub loading_frames: u64,
    pub scroll: u16,
    pub input_scroll: u16,
    pub chat_history: Vec<ChatMessage>,
    pub rendered_history: Vec<Vec<Line<'static>>>,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,
    pub scrolled_manually: bool,
    pub current_task: Option<tokio::task::JoinHandle<()>>,
    pub current_task_id: u64,
    pub thinking_start_time: Option<Instant>,
}

impl App {
    pub fn new() -> App {
        let os = if cfg!(target_os = "windows") { "Windows" } else { "Linux/macOS" };
        let shell = if cfg!(target_os = "windows") { "PowerShell" } else { "sh" };

        let system_prompt = format!(r#"You are AI-Mentor, a world-class coding agent and terminal expert powered by Mistral.
Your goal is to solve the user's request by exploring the codebase, reading files, and executing commands.

### ENVIRONMENT INFO
- **Operating System:** {}
- **Primary Shell:** {}
"#, os, shell);

        let system_msg = ChatMessage::new(
            MessageRole::System, 
            system_prompt.to_string()
        );

        App {
            input: Input::default(),
            ai_response: String::with_capacity(1024 * 100), 
            rendered_ai_response: Vec::new(),
            ai_response_lines: 0,
            is_loading: false,
            pasting: false,
            loading_frames: 0,
            scroll: 0,
            input_scroll: 0,
            chat_history: vec![system_msg],
            rendered_history: Vec::new(),
            prompt_history: Vec::new(),
            history_index: None,
            scrolled_manually: false,
            current_task: None,
            current_task_id: 0,
            thinking_start_time: None,
        }
    }

    pub fn tick(&mut self) {
        if self.is_loading {
            self.loading_frames = self.loading_frames.wrapping_add(1);
        }
    }

    pub fn get_elapsed_thinking_time(&self) -> String {
        if let Some(start) = self.thinking_start_time {
            let elapsed = start.elapsed().as_secs();
            let hours = elapsed / 3600;
            let minutes = (elapsed % 3600) / 60;
            let seconds = elapsed % 60;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            "00:00:00".to_string()
        }
    }

    pub fn sync_history(&mut self, new_history: Vec<ChatMessage>) {
        self.chat_history = new_history;
        while self.rendered_history.len() < self.chat_history.len() {
            let idx = self.rendered_history.len();
            let msg = &self.chat_history[idx];
            if msg.role == MessageRole::Assistant {
                self.rendered_history.push(crate::ui::render_markdown(&msg.content));
            } else {
                self.rendered_history.push(Vec::new()); // Non-assistant messages are rendered directly in ui.rs for now
            }
        }
    }

    pub fn clear_ai_response(&mut self) {
        self.ai_response.clear();
        self.rendered_ai_response.clear();
        self.ai_response_lines = 0;
    }

    pub fn update_ai_response(&mut self, chunk: &str) {
        if self.ai_response.is_empty() && !chunk.is_empty() {
            self.ai_response_lines = 1;
        }
        self.ai_response.push_str(chunk);
        self.ai_response_lines += chunk.chars().filter(|&c| c == '\n').count();
        // Update rendered response incrementally
        self.rendered_ai_response = crate::ui::render_markdown(&self.ai_response);
    }

    pub fn auto_scroll(&mut self, height: u16) {
        if !self.scrolled_manually {
            if self.ai_response_lines as u16 > height {
                self.scroll = (self.ai_response_lines as u16).saturating_sub(height);
            }
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.scrolled_manually = false;
    }

    pub fn insert_text(&mut self, text: &str) {
        let current_val = self.input.value();
        let cursor_pos = self.input.cursor();

        let mut new_val = String::with_capacity(current_val.len() + text.len());

        let (left, right) = current_val.split_at(cursor_pos);
        new_val.push_str(left);
        new_val.push_str(text);
        new_val.push_str(right);

        let new_cursor = cursor_pos + text.chars().count();
        self.input = Input::new(new_val).with_cursor(new_cursor);
    }
}
