use anyhow::Result;
use ollama_rs::{
    generation::chat::{request::ChatMessageRequest, ChatMessage, MessageRole},
    Ollama,
    models::ModelOptions,
};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use futures::StreamExt;
use crate::mentor::{Config, MentorEvent};
use crate::tools;

pub struct Executor {
    ollama: Ollama,
    model: String,
    config: Config,
}

impl Executor {
    pub fn new() -> Self {
        let config = Config::load();
        let (model, api_base, _) = config.get_current_model();
        let url = url::Url::parse(&api_base).unwrap_or_else(|_| url::Url::parse("http://localhost:11434").unwrap());
        Self {
            ollama: Ollama::from_url(url),
            model,
            config,
        }
    }

    pub async fn execute_step(&self, task_id: u64, step_description: &str, project_context: &str, os: &str, shell: &str, tx: mpsc::Sender<MentorEvent>) -> Result<String> {
        let mut messages = vec![
            ChatMessage::new(MessageRole::System, format!(
                r#"You are AI-MENTOR CORE: AN AUTONOMOUS TOOL ENGINE.
Your goal is to execute the following TASK by emitting TOOL CALLS.

### ENVIRONMENT
- OS: {}
- Shell: {}

### RELEVANT CONTEXT
{}

### SYSTEM RULES
1. Output ONLY valid XML tool tags in the format: <tool code="tool_name">arguments</tool>
2. Stay strictly within the provided technical context.
3. Prioritize NATIVE tools (e.g., `list_processes`) over generic shell commands.
4. FORBID manual process sorting via `run_shell_command`. YOU MUST USE NATIVE `list_processes`.
5. FORBID using PowerShell formatting cmdlets (Format-Table, Format-List, Format-Wide) as they break output capture.
6. Use `list_processes mem` for memory/RAM sorting or `list_processes cpu` for CPU sorting.
7. If the task is completed, provide a concise technical summary and [DONE].

TASK:
"{}"
"#,
                os, shell, project_context, step_description
            ))
        ];

        let mut turn_count = 0;
        let max_turns = 6;
        let timeout_secs = self.config.request_timeout_seconds;

        loop {
            if turn_count >= max_turns {
                let msg = "Executor hit max turns.".to_string();
                return Ok(msg);
            }
            turn_count += 1;

            // Context Management: Keep only system prompt and last 2 exchanges (4 messages) + any new ones
            if messages.len() > 6 {
                let system_msg = messages.remove(0);
                messages = messages.split_off(messages.len() - 4);
                messages.insert(0, system_msg);
            }

            let mut stream = match timeout(
                Duration::from_secs(timeout_secs),
                self.ollama.send_chat_messages_stream(
                    ChatMessageRequest::new(self.model.clone(), messages.clone())
                        .options(ModelOptions::default().temperature(0.0))
                )
            ).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => return Err(anyhow::anyhow!("Ollama error: {}", e)),
                Err(_) => return Err(anyhow::anyhow!("Ollama request timed out")),
            };

            let mut response_text = String::new();
            let mut detected_hallucination = false;

            let mut last_tool_call: Option<String> = None;

            while let Some(res) = timeout(Duration::from_secs(10), stream.next()).await.unwrap_or(None) {
                match res {
                    Ok(res) => {
                        let content = res.message.content;
                        response_text.push_str(&content);

                        if is_hallucination(&response_text) {
                            detected_hallucination = true;
                            let _ = tx.send(MentorEvent::Chunk(task_id, "\n[HALLUCINATION DETECTED - TERMINATING STREAM]\n".to_string())).await;
                            break; 
                        }

                        let _ = tx.send(MentorEvent::Chunk(task_id, content)).await;
                        if res.done { break; }
                    }
                    Err(e) => return Err(anyhow::anyhow!("Stream error: {:?}", e)),
                }
            }

            if detected_hallucination {
                if let Some(pos) = response_text.to_lowercase().find("question:") {
                    response_text.truncate(pos);
                } else if let Some(pos) = response_text.to_lowercase().find("let w(q)") {
                    response_text.truncate(pos);
                }
            }

            messages.push(ChatMessage::new(MessageRole::Assistant, response_text.clone()));

            // Dual Check: Some models output [DONE] before the tool tag
            let is_finished = response_text.to_uppercase().contains("[DONE]");

            // Parse Tools
            let tool_regex = regex::Regex::new(r#"(?s)<tool\s+code="([^"]+)">\s*(.*?)\s*</tool>"#).unwrap();
            let mut tool_outputs = String::new();
            let mut used_info_tool = false;
            
            for cap in tool_regex.captures_iter(&response_text) {
                let tool_attr = &cap[1];
                let inner_args = &cap[2];
                
                // Robust Tool Name Extraction (Handle <tool code="list_processes mem">)
                let (tool, args_full) = if let Some((t, extra)) = tool_attr.split_once(' ') {
                    (t.trim(), format!("{} {}", extra.trim(), inner_args.trim()))
                } else {
                    (tool_attr.trim(), inner_args.trim().to_string())
                };

                if tool == "list_processes" || tool == "list_directory" || tool == "read_file" {
                    used_info_tool = true;
                }

                // Recursive Hallucination Prevention: Strip redundant tool names from args
                let sanitized_args = sanitize_tool_args(tool, &args_full);

                // Loop Detection: If we call the exact same tool with same args twice, stop.
                let current_call = format!("{}({})", tool, sanitized_args.trim());
                if let Some(ref last) = last_tool_call {
                    if last == &current_call {
                        return Err(anyhow::anyhow!("Executor detected an infinite tool loop with: {}", current_call));
                    }
                }
                last_tool_call = Some(current_call);

                let _ = tx.send(MentorEvent::Chunk(task_id, format!("\n[Executing tool: {}]\n", tool))).await;

                let output = match tool {
                    "list_directory" => tools::list_directory(sanitized_args.trim()),
                    "read_file" => tools::read_file(sanitized_args.trim(), None, None),
                    "list_processes" => {
                        let arg = sanitized_args.trim().to_lowercase();
                        let sort_by = if arg.contains("mem") || arg.contains("ws") || arg.contains("rss") || arg.contains("working") {
                            Some("mem")
                        } else if arg.contains("cpu") {
                            Some("cpu")
                        } else {
                            None
                        };
                        tools::list_processes(sort_by).await
                    },
                    "write_file" => {
                        if let Some((path, content)) = sanitized_args.trim().split_once('\n') {
                            tools::write_file(path.trim(), content)
                        } else {
                            "Error: write_file format invalid".to_string()
                        }
                    },
                    "run_shell_command" => tools::run_shell_command(sanitized_args.trim()).await,
                    "get_system_info" => tools::get_system_info().await,
                    _ => format!("Unknown tool: {}. VALID TOOLS: list_directory, read_file, write_file, run_shell_command, list_processes, get_system_info", tool),
                };
                
                let lines: Vec<&str> = output.lines().collect();
                let summary = if lines.len() > 30 {
                    format!("{}\n... (and {} more lines)", lines[..30].join("\n"), lines.len() - 30)
                } else {
                    output.clone()
                };
                let _ = tx.send(MentorEvent::Chunk(task_id, format!("> {}\n", summary))).await;
                tool_outputs.push_str(&format!("Tool Output ({}):\n{}\n", tool, output));
            }

            // AUTO-TERMINATE for info tools to prevent "Thinking" loops
            let force_done = used_info_tool && response_text.len() > 10;

            // Check for termination AFTER tools or via DUAL CHECK
            if is_finished || response_text.to_uppercase().contains("[DONE]") || force_done {
                let mut final_response = response_text.clone();
                if !final_response.to_uppercase().contains("[DONE]") {
                     let _ = tx.send(MentorEvent::Chunk(task_id, "\n[SYSTEM: Auto-completing task]".to_string())).await;
                     final_response.push_str(" [DONE]");
                }
                return Ok(final_response);
            }

            if !tool_outputs.is_empty() {
                let model_feedback = if tool_outputs.len() > 2000 {
                    format!("{}... (Truncated to 2000 chars)\n[SYSTEM: Output was very large and was truncated for efficiency.]", &tool_outputs[..2000])
                } else {
                    tool_outputs
                };
                messages.push(ChatMessage::new(MessageRole::User, model_feedback));
            } else {
                if response_text.trim().len() > 5 {
                    return Ok(response_text);
                }
                
                let nudge = if detected_hallucination {
                    "Technical error in previous output stream. RESTARTING TASK: Please provide the required tool call(s) now using the format: <tool code=\"tool_name\">arguments</tool>\nVALID TOOLS: list_directory, read_file, write_file, run_shell_command, list_processes, get_system_info".to_string()
                } else {
                    "Please conclude your response or output [DONE].".to_string()
                };
                let _ = tx.send(MentorEvent::Chunk(task_id, format!("\n[System: {}]\n", nudge))).await;
                
                // If hallucination, purge flawed messages to prevent repetitive drift
                if detected_hallucination {
                    let system_msg = messages.remove(0);
                    messages.clear();
                    messages.push(system_msg);
                }
                
                messages.push(ChatMessage::new(MessageRole::User, nudge));
            }
        }
    }
}

pub fn sanitize_tool_args(tool: &str, args: &str) -> String {
    let mut s = args.trim().to_string();
    
    // 0. Handle double enclosure (model quotes the whole tool call)
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s = s[1..s.len()-1].trim().to_string();
    }

    // 1. Strip redundant tool name prefix (Recursive Hallucination)
    let prefixes = [
        (format!("{} ", tool), ""),
        (format!("{}\"", tool), "\""),
        (format!("{}(", tool), ")"),
        (format!("{} '", tool), "'"),
    ];
    
    for (prefix, suffix) in prefixes {
        if s.starts_with(&prefix) {
            s = s[prefix.len()..].trim().to_string();
            if !suffix.is_empty() && s.ends_with(suffix) {
                s = s[..s.len() - suffix.len()].trim().to_string();
            }
            // Strip any remaining quotes from the arguments themselves
            if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
                s = s[1..s.len()-1].to_string();
            }
            break;
        }
    }
    s.trim().to_string()
}

pub fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();
    
    // Core math/benchmark hallucinations
    if lower.contains("question:") || 
       lower.contains("answer:") ||
       lower.contains("suppose ") ||
       lower.contains("let's say") ||
       lower.contains("let w(") || 
       lower.contains("w(q)") ||
       lower.contains("w(-6)") ||
       lower.contains("h*x") ||
       lower.contains("y*h") ||
       lower.contains(" 2*q") ||
       lower.contains("3*q") ||
       lower.contains("4*q") ||
       lower.contains("answer: 0") ||
       lower.contains("answer: 1") {
        return true;
    }

    // Recursive tool call hallucinations (model repeating tool names in content)
    // We only trigger this if it looks like conversational filler + tool name
    let tool_names = ["run_shell_command", "list_directory", "read_file", "write_file", "list_processes"];
    for name in tool_names {
        if lower.contains(&format!("i will use {}", name)) || 
           lower.contains(&format!("using the {} tool", name)) ||
           lower.contains(&format!("calling {}", name)) {
            return true;
        }
    }

    false
}
