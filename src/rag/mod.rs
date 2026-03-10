use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::fs;
use once_cell::sync::Lazy;
use ignore::WalkBuilder;

pub mod vector_db;

// --- High-Performance In-Memory Index ---

struct FileEntry {
    mtime: SystemTime,
    summary: String,
    tokens: HashSet<String>,
}

struct ProjectIndex {
    files: HashMap<PathBuf, FileEntry>,
    structure_context: String,
    last_refresh: SystemTime,
}

impl ProjectIndex {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
            structure_context: String::new(),
            last_refresh: UNIX_EPOCH,
        }
    }
}

static INDEX: Lazy<RwLock<ProjectIndex>> = Lazy::new(|| RwLock::new(ProjectIndex::new()));

/// Retrieves context instantly from memory, refreshing only changed files.
pub async fn get_context(query: &str) -> String {
    let query_lower = query.to_lowercase();
    
    // 1. Fast Path: System queries
    let system_keywords = ["process", "running", "list all", "cpu", "ram", "memory", "system"];
    let is_system_query = system_keywords.iter().any(|k| query_lower.contains(k));
    if is_system_query && !query_lower.contains("code") && !query_lower.contains("file") {
        return "### PROJECT CONTEXT ###\n(Minimal context provided for system-related query)".to_string();
    }

    // 2. Refresh Index (if needed)
    refresh_index_if_needed();

    // 3. Query In-Memory Index
    let index = INDEX.read().unwrap();
    let mut context = String::from("### PROJECT CONTEXT ###\n");
    
    // Always include project structure (cached)
    context.push_str(&index.structure_context);

    // Identify relevant keywords
    let keywords: HashSet<_> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|w| w.to_string())
        .collect();

    // Find relevant files via in-memory token matching
    let mut relevant_files = Vec::new();
    for (path, entry) in &index.files {
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        
        let is_relevant = keywords.is_empty() 
            || keywords.iter().any(|k| filename.contains(k))
            || entry.tokens.intersection(&keywords).next().is_some()
            || (query_lower.contains("code") && path.extension().map_or(false, |e| e == "rs"));

        if is_relevant {
            relevant_files.push((path, &entry.summary));
        }
    }

    // Sort to keep context deterministic and limit size
    relevant_files.sort_by(|a, b| a.0.cmp(b.0));
    
    if !relevant_files.is_empty() {
        context.push_str("\nRELEVANT SOURCE SNIPPETS (src/):\n");
        for (path, summary) in relevant_files.into_iter().take(5) {
            context.push_str(&format!("\nFile: {}\n```rust\n{} ...\n```\n", path.display(), summary));
        }
    } else {
        context.push_str("(No specific source snippets matched query keywords)\n");
    }

    // Special case for Cargo.toml
    if query_lower.contains("dep") || query_lower.contains("cargo") {
         if let Ok(content) = fs::read_to_string("Cargo.toml") {
             context.push_str("\nCargo.toml:\n```toml\n");
             context.push_str(&content.lines().take(20).collect::<Vec<_>>().join("\n"));
             context.push_str("\n```\n");
         }
    }

    context
}

fn refresh_index_if_needed() {
    // Check TTL (e.g., 2 seconds) to prevent spamming stats in a tight loop
    if let Ok(index) = INDEX.read() {
        if index.last_refresh.elapsed().unwrap_or(Duration::from_secs(0)) < Duration::from_secs(2) {
            return;
        }
    }

    // Upgrade to write lock
    let mut index = INDEX.write().unwrap();
    index.last_refresh = SystemTime::now();

    // Re-build Structure & Update Files
    let mut structure = String::from("\nSTRUCTURE:\n");
    let walker = WalkBuilder::new(".")
        .hidden(false) // Show hidden files (like .env)
        .git_ignore(true) // Respect .gitignore
        .max_depth(Some(2))
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();
                let depth = entry.depth();
                
                // Update Structure String
                if depth > 0 && depth <= 2 {
                    let indent = "  ".repeat(depth);
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let suffix = if path.is_dir() { "/" } else { "" };
                    structure.push_str(&format!("{}{}{}\n", indent, name, suffix));
                }

                // Update File Index (only for .rs files in src for now)
                if path.starts_with("./src") || path.starts_with("src") {
                    if let Some(ext) = path.extension() {
                        if ext == "rs" {
                            if let Ok(metadata) = fs::metadata(path) {
                                let mtime = metadata.modified().unwrap_or(UNIX_EPOCH);
                                
                                // Check if we need to (re)read this file
                                let needs_update = match index.files.get(path) {
                                    Some(entry) => entry.mtime != mtime,
                                    None => true,
                                };

                                if needs_update {
                                    if let Ok(content) = fs::read_to_string(path) {
                                        let summary: String = content.lines().take(20).collect::<Vec<_>>().join("\n");
                                        let tokens: HashSet<String> = content
                                            .to_lowercase()
                                            .split_whitespace()
                                            .filter(|w| w.len() > 3)
                                            .map(|w| w.to_string())
                                            .collect();
                                        
                                        index.files.insert(path.to_path_buf(), FileEntry {
                                            mtime,
                                            summary,
                                            tokens,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }
    index.structure_context = structure;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_context() {
        // Force a refresh first
        refresh_index_if_needed();
        
        let context = get_context("anything").await;
        assert!(context.contains("PROJECT CONTEXT"));
        assert!(context.contains("STRUCTURE:"));
    }
}


