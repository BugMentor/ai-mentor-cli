use std::fs;
use std::collections::HashSet;

pub async fn get_context(query: &str) -> String {
    let files = vec![
        "src/main.rs",
        "src/app.rs",
        "src/ui.rs",
        "src/mentor.rs",
        "src/rag.rs",
        "Cargo.toml"
    ];
    let mut scored_files = Vec::new();
    let query_words: HashSet<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.replace(|c: char| !c.is_alphanumeric(), ""))
        .collect();

    for path in files {
        if let Ok(content) = fs::read_to_string(path) {
            let mut score = 0;
            let file_content_lower = content.to_lowercase();
            for word in &query_words {
                if word.len() > 2 && file_content_lower.contains(word) {
                    score += 1;
                }
            }
            scored_files.push((score, path, content));
        }
    }
    scored_files.sort_by(|a, b| b.0.cmp(&a.0));
    scored_files
        .iter()
        .take(2)
        .map(|(_, path, content)| format!("\n--- FILE: {} ---\n{}\n", path, content))
        .collect::<Vec<_>>()
        .join("\n")
}
