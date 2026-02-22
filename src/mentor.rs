use ollama_rs::{ generation::chat::{ request::ChatMessageRequest, ChatMessage }, Ollama };
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use crate::rag::get_context;

pub enum MentorEvent {
    Chunk(String),
    Finished,
    Error(String),
}

pub async fn ask_mentor(prompt: String, tx: mpsc::Sender<MentorEvent>) {
    // Explicitly pointing to your running server
    let ollama = Ollama::new("http://127.0.0.1".to_string(), 11434);
    let context = get_context(&prompt).await;

    let system_prompt =
        format!("You are an expert mentor. Use this local context:\n{}\nUse <file name=\"filename\">content</file> tags.", context);

    // Using the exact name from your 'ollama list'
    let messages = vec![ChatMessage::system(system_prompt), ChatMessage::user(prompt)];

    let mut stream = match
        ollama.send_chat_messages_stream(
            ChatMessageRequest::new("mistral:latest".to_string(), messages)
        ).await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(MentorEvent::Error(format!("Ollama Error: {}", e))).await;
            return;
        }
    };

    while let Some(res) = stream.next().await {
        if let Ok(res) = res {
            let msg = res.message;
            let _ = tx.send(MentorEvent::Chunk(msg.content)).await;
        }
    }
    let _ = tx.send(MentorEvent::Finished).await;
}
