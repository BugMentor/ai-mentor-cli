use ollama_rs::{
    generation::completion::request::GenerationRequest,
    generation::options::GenerationOptions,
    Ollama,
};
use tokio::sync::mpsc;
use futures::StreamExt;

pub enum MentorEvent {
    Chunk(String),
    Finished,
    Error(String),
}

pub async fn ask_mentor(prompt: String, tx: mpsc::Sender<MentorEvent>) {
    let ollama = Ollama::default();

    let options = GenerationOptions::default()
        .temperature(0.0)
        .top_k(15)
        .top_p(0.5)
        .num_predict(4096);

    let mut stream = match
        ollama.generate_stream(
            GenerationRequest::new("mistral".to_string(), prompt).options(options)
        ).await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(MentorEvent::Error(e.to_string())).await;
            return;
        }
    };

    while let Some(res) = stream.next().await {
        match res {
            Ok(responses) => {
                for resp in responses {
                    let _ = tx.send(MentorEvent::Chunk(resp.response)).await;
                }
            }
            Err(e) => {
                let _ = tx.send(MentorEvent::Error(e.to_string())).await;
                break;
            }
        }
    }
    let _ = tx.send(MentorEvent::Finished).await;
}
