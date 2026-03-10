use ollama_rs::{
    generation::chat::{request::ChatMessageRequest, ChatMessage, MessageRole},
    Ollama,
    models::ModelOptions,
};
use crate::mentor::Config;
use anyhow::Result;

pub struct Reflector {
    ollama: Ollama,
    model: String,
}

impl Reflector {
    pub fn new() -> Self {
        let config = Config::load();
        let (model, api_base, _) = config.get_current_model();
        let url = url::Url::parse(&api_base).unwrap_or_else(|_| url::Url::parse("http://localhost:11434").unwrap());
        Self {
            ollama: Ollama::from_url(url),
            model,
        }
    }

    pub async fn reflect_on_failure(&self, task: &str, error_log: &str) -> Result<String> {
        let prompt = format!(
            r#"You are a debugging expert.
The following task failed:
"{}"

ERROR LOG:
{}

INSTRUCTIONS:
1. Analyze the error log.
2. Determine the root cause.
3. Propose a specific, corrected plan to fix this error.
4. Keep it concise.
"#,
            task, error_log
        );

        let msgs = vec![ChatMessage::new(MessageRole::User, prompt)];
        
        let res = self.ollama.send_chat_messages(
             ChatMessageRequest::new(self.model.clone(), msgs)
                 .options(ModelOptions::default().temperature(0.1))
        ).await?;

        Ok(res.message.content)
    }
}
