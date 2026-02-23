use std::fs;

pub async fn get_context(_query: &str) -> String {
    let mut context = String::new();
    // Simple RAG: Read all .rs files in src to give the AI project awareness
    if let Ok(entries) = fs::read_dir("src") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(&path) {
                    context.push_str(&format!("\n--- File: {:?} ---\n{}\n", path, content));
                }
            }
        }
    }
    context
}
