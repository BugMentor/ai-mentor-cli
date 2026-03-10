use ollama_rs::generation::chat::{ChatMessage, MessageRole};
use anyhow::Result;
use crate::mentor::Config;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::models::ModelOptions;

pub struct Planner {
    ollama: Ollama,
    model: String,
}

impl Planner {
    pub fn new() -> Self {
        let config = Config::load();
        let (model, api_base, _) = config.get_current_model();
        let url = url::Url::parse(&api_base).unwrap_or_else(|_| url::Url::parse("http://localhost:11434").unwrap());
        Self {
            ollama: Ollama::from_url(url),
            model,
        }
    }

    pub async fn create_plan(&self, task: &str, context_summary: &str, os: &str, shell: &str) -> Result<Vec<String>> {
        let prompt = format!(
            r#"You are AI-MENTOR PLANNER: A DETERMINISTIC TASK DECOMPOSER.
Decompose the USER TASK into a MINIMAL sequence of steps using the available TOOLS.

### AVAILABLE TOOLS
* TOOL: list_directory (usage: provide path)
* TOOL: read_file (usage: provide path)
* TOOL: write_file (usage: provide path and content separated by newline)
* TOOL: run_shell_command (usage: provide full shell command)
* TOOL: list_processes (usage: ONLY way to view processes, use 'cpu' or 'mem' args)

### ENVIRONMENT
- OS: {}
- Shell: {}

### RELEVANT CONTEXT
{}

### STRICT RULES
1. Provide a MAX of 2 steps for any task. 
2. For informational tasks (listing, reading, searching, listing processes), YOU MUST USE EXACTLY 1 step.
3. FORBID using `run_shell_command` to list processes. YOU MUST USE `list_processes`.
4. Each step must be a goal-oriented action (e.g., "List all processes", "Read the main.rs file"). 
5. DO NOT include literal shell commands, code blocks, or specific tool arguments in the plan steps.
6. Provide ONLY a numbered list of steps, no headers or conversational filler.

USER TASK:
"{}"
"#,
            os, shell, context_summary, task
        );

        let msgs = vec![ChatMessage::new(MessageRole::User, prompt)];
        
        let res = self.ollama.send_chat_messages(
             ChatMessageRequest::new(self.model.clone(), msgs)
                 .options(ModelOptions::default().temperature(0.0))
        ).await?;

        let content = res.message.content;
        let steps = parse_steps(&content);
            
        Ok(steps)
    }
}

pub fn parse_steps(content: &str) -> Vec<String> {
    content.lines()
        .filter(|l| {
            let trimmed = l.trim();
            // Relaxed: Must be at least 5 chars and not look like a header
            trimmed.len() > 5 && !trimmed.ends_with(':')
        })
        .map(|l| {
            let s = l.trim();
            // Strip common prefixes like "1. ", "- ", "* ", etc.
            if let Some(pos) = s.find(|c: char| c.is_alphabetic()) {
                s[pos..].to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}
// ... (tests omitted for brevity but they are expected to stay in the file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_steps() {
        let content = "1. List all processes\n- Read file src/main.rs\n* Run build";
        let steps = parse_steps(content);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "List all processes");
        assert_eq!(steps[1], "Read file src/main.rs");
        assert_eq!(steps[2], "Run build");
    }

    #[test]
    fn test_parse_steps_with_headers() {
        let content = "Plan:\n1. Step one\nMore info:\n2. Step two";
        let steps = parse_steps(content);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], "Step one");
        assert_eq!(steps[1], "Step two");
    }

    #[test]
    fn test_parse_steps_short() {
        let content = "1. a\n2. Longer step";
        let steps = parse_steps(content);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0], "Longer step");
    }
}
