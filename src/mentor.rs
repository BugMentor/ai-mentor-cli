use ollama_rs::{
    generation::chat::{request::ChatMessageRequest, ChatMessage, MessageRole},
    models::ModelOptions,
    Ollama,
};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use regex::Regex;
use crate::tools;
use std::sync::Arc;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub api_base: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub default_model: String,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u64,
    pub models: HashMap<String, ModelConfig>,
}

fn default_timeout() -> u64 {
    120
}

impl Config {
    pub fn load() -> Self {
        let content = std::fs::read_to_string("config.toml").unwrap_or_default();
        toml::from_str(&content).unwrap_or_else(|_| Config {
            default_model: "mistral".to_string(),
            request_timeout_seconds: 120,
            models: HashMap::new(),
        })
    }

    pub fn get_current_model(&self) -> (String, String, u64) {

        let timeout = self.request_timeout_seconds;
        if let Some(m) = self.models.get(&self.default_model) {
            (m.name.clone(), m.api_base.clone(), timeout)
        } else {
            ("mistral".to_string(), "http://localhost:11434".to_string(), timeout)
        }
    }
}

#[derive(Debug)]
pub enum MentorEvent {
    Chunk(u64, String),
    Finished(u64),
    Error(u64, String),
    HistoryUpdate(u64, Vec<ChatMessage>), // New event to sync history
}

pub async fn ask_mentor_chat(task_id: u64, mut messages: Vec<ChatMessage>, context: Option<String>, tx: mpsc::Sender<MentorEvent>) {
    let config = Config::load();
    let (model_name, api_base, timeout_secs) = config.get_current_model();
    
    let url = url::Url::parse(&api_base).unwrap_or_else(|_| url::Url::parse("http://localhost:11434").unwrap());
    let ollama = Ollama::from_url(url);
    let max_turns = 10;
    let mut current_turn = 0;

    let tool_regex = Arc::new(Regex::new(r#"(?s)<tool\s+code="([^"]+)">\s*(.*?)\s*</tool>"#).unwrap());

    // Keep history manageable: System + Last 5 turns (User/Assistant pairs)
    if messages.len() > 11 {
        let system_msg = messages[0].clone();
        let recent = messages.iter().skip(messages.len() - 10).cloned().collect::<Vec<_>>();
        messages = vec![system_msg];
        messages.extend(recent);
    }

    loop {
        if current_turn >= max_turns {
            let _ = tx.send(MentorEvent::Chunk(task_id, "\n[System: Max turns reached, stopping agent loop.]\n".to_string())).await;
            break;
        }
        current_turn += 1;

        let options = ModelOptions::default()
            .temperature(0.0)
            .top_k(20)
            .top_p(0.9)
            .repeat_penalty(1.1)
            .num_ctx(4096)
            .num_predict(4096); 

        let request_timeout = Duration::from_secs(timeout_secs);
        
        // Prepare current request with context (only if it's the first turn of the loop)
        let mut request_messages = messages.clone();
        if let (0, Some(ctx)) = (current_turn - 1, &context) {
            if let Some(last_msg) = request_messages.last_mut() {
                if last_msg.role == MessageRole::User {
                    last_msg.content = format!("CONTEXT:\n{}\n\nPROMPT: {}", ctx, last_msg.content);
                }
            }
        }

        let stream_result = timeout(
            request_timeout,
            ollama.send_chat_messages_stream(
                ChatMessageRequest::new(model_name.clone(), request_messages).options(options)
            )
        ).await;


        let mut stream = match stream_result {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let _ = tx.send(MentorEvent::Error(task_id, format!("Ollama Error: {}", e))).await;
                return;
            }
            Err(_) => {
                let _ = tx.send(MentorEvent::Error(task_id, format!("Ollama request timed out after {}s. Try increasing 'request_timeout_seconds' in config.toml.", timeout_secs))).await;
                return;
            }
        };

        let mut full_response = String::new();
        let first_chunk_timeout = Duration::from_secs(timeout_secs);
        let turn_timeout = Duration::from_secs(timeout_secs);

        let mut stream_started = false;
        
        loop {
            let next_chunk = timeout(
                if stream_started { turn_timeout } else { first_chunk_timeout },
                stream.next()
            ).await;

            match next_chunk {
                Ok(Some(Ok(response))) => {
                    stream_started = true;
                    let c = response.message.content;
                    full_response.push_str(&c);
                    let _ = tx.send(MentorEvent::Chunk(task_id, c)).await;
                    if response.done { break; }
                }
                Ok(Some(Err(e))) => {
                    let _ = tx.send(MentorEvent::Error(task_id, format!("Stream Error: {:?}", e))).await;
                    return;
                }
                Ok(None) => break,
                Err(_) => {
                    let msg = if stream_started {
                        "Mistral stalled during generation (timeout)."
                    } else {
                        "Mistral failed to start responding (timeout). Is Ollama running?"
                    };
                    let _ = tx.send(MentorEvent::Error(task_id, msg.to_string())).await;
                    return;
                }
            }
        }

        if full_response.is_empty() {
             let _ = tx.send(MentorEvent::Error(task_id, "Mistral failed to respond in time (empty response).".to_string())).await;
             return;
        }

        // Add the Assistant's response to history
        messages.push(ChatMessage::new(MessageRole::Assistant, full_response.clone()));

        // Parse for tools
        let captures = parse_tool_calls_with_regex(&full_response, &tool_regex);

        if captures.is_empty() {
            // No tools called, we are done with this turn
            // If it's a substantive response and no tool was called, we can stop the loop early
            if full_response.len() > 100 {
                break;
            }
            
            // Otherwise, if it was very short/empty and didn't call tools, 
            // maybe it's stuck or just chatting. We'll give it one nudge.
            if current_turn < max_turns {
                let nudge = "Please continue or conclude your task.".to_string();
                messages.push(ChatMessage::new(MessageRole::User, nudge));
                continue;
            }
            break;
        }

        let mut tool_tasks = FuturesUnordered::new();
        
        for (tool_name, tool_args) in captures {
            let _ = tx.send(MentorEvent::Chunk(task_id, format!("\n\n[Executing tool: {}]\n", tool_name))).await;
            
            let tool_name_c = tool_name.clone();
            let tool_args_c = tool_args.clone();

            tool_tasks.push(tokio::task::spawn(async move {
                let output = match tool_name_c.as_str() {
                    "list_directory" => tools::list_directory(tool_args_c.trim()),
                    "read_file" => tools::read_file(tool_args_c.trim(), None, None),
                    "find_files" => tools::find_files(tool_args_c.trim()),
                    "write_file" => {
                        let args = tool_args_c.trim();
                        if let Some((path, content)) = args.split_once('\n') {
                            tools::write_file(path.trim(), content)
                        } else {
                            "Error: write_file expects format 'path\\ncontent'".to_string()
                        }
                    },
                    "replace" => {
                        let args = tool_args_c.trim();
                        let path_part = args.split("OLD:").next().unwrap_or("").replace("PATH:", "").trim().to_string();
                        let rest = args.split("OLD:").nth(1).unwrap_or("");
                        let old_part = rest.split("NEW:").next().unwrap_or("").trim();
                        let new_part = rest.split("NEW:").nth(1).unwrap_or("").trim();
                        
                        if path_part.is_empty() || old_part.is_empty() {
                            "Error: replace expects PATH: ..., OLD: ..., NEW: ... format.".to_string()
                        } else {
                            tools::replace_text(&path_part, old_part, new_part)
                        }
                    },
                    "run_shell_command" => tools::run_shell_command(tool_args_c.trim()).await,
                    "list_processes" => {
                        let arg = tool_args_c.trim().to_lowercase();
                        tools::list_processes(if arg.is_empty() { None } else { Some(&arg) }).await
                    },
                    "grep_search" => tools::grep_search(tool_args_c.trim(), None, None),
                    "ripgrep" => tools::ripgrep(tool_args_c.trim()),
                    "glob" => tools::glob_search(tool_args_c.trim()),
                    _ => format!("Error: Unknown tool '{}'", tool_name_c),
                };
                (tool_name_c, output)
            }));
        }

        let mut tool_outputs = String::new();
        while let Some(res) = tool_tasks.next().await {
            if let Ok((tool_name, output)) = res {
                // Truncate output if it's too large for the model's context
                let max_lines = 100;
                let max_chars = 4000;
                let truncated_output = if output.lines().count() > max_lines {
                    let truncated: String = output.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                    format!("{}\n... (Output truncated: too many lines)", truncated)
                } else if output.len() > max_chars {
                    format!("{}\n... (Output truncated: too long)", &output[..max_chars])
                } else {
                    output.clone()
                };

                tool_outputs.push_str(&format!("TOOL OUTPUT ({}):\n{}\n", tool_name, truncated_output));
                
                let summary = output.lines().take(5).collect::<Vec<_>>().join("\n");
                let _ = tx.send(MentorEvent::Chunk(task_id, format!("> {}\n", summary))).await;
                if output.lines().count() > 5 {
                    let _ = tx.send(MentorEvent::Chunk(task_id, format!("... ({} lines total)\n", output.lines().count()))).await;
                }
            }
        }
        
        // Feed tool output back to model
        messages.push(ChatMessage::new(MessageRole::User, tool_outputs));
    }

    let _ = tx.send(MentorEvent::HistoryUpdate(task_id, messages)).await;
    let _ = tx.send(MentorEvent::Finished(task_id)).await;
}

fn parse_tool_calls_with_regex(response: &str, regex: &Regex) -> Vec<(String, String)> {
    let mut captures = Vec::new();
    for cap in regex.captures_iter(response) {
        captures.push((cap[1].to_string(), cap[2].to_string()));
    }
    captures
}

#[cfg(test)]
fn parse_tool_calls(response: &str) -> Vec<(String, String)> {
    let tool_regex = Regex::new(r#"(?s)<tool\s+code="([^"]+)">\s*(.*?)\s*</tool>"#).unwrap();
    parse_tool_calls_with_regex(response, &tool_regex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_calls_single() {
        let response = "I will list the directory.\n<tool code=\"list_directory\">.</tool>";
        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "list_directory");
        assert_eq!(calls[0].1, ".");
    }

    #[test]
    fn test_parse_tool_calls_multiple() {
        let response = "First list:\n<tool code=\"list_directory\">src</tool>\nThen read:\n<tool code=\"read_file\">src/main.rs</tool>";
        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "list_directory");
        assert_eq!(calls[0].1, "src");
        assert_eq!(calls[1].0, "read_file");
        assert_eq!(calls[1].1, "src/main.rs");
    }

    #[test]
    fn test_parse_tool_calls_multiline_args() {
        let response = "<tool code=\"write_file\">src/test.rs\nfn main() {}\n</tool>";
        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "write_file");
        assert!(calls[0].1.contains("fn main() {}"));
    }
    
    #[test]
    fn test_parse_no_calls() {
        let response = "Just chatting.";
        let calls = parse_tool_calls(response);
        assert!(calls.is_empty());
    }
}
