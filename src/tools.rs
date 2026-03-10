use std::fs;
use std::path::Path;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use walkdir::WalkDir;
use glob::glob;
use regex::Regex;
use ignore::WalkBuilder;

// --- Tool Execution Logic ---

pub fn list_directory(dir_path: &str) -> String {
    let path = Path::new(dir_path);
    if !path.exists() {
        return format!("Error: Directory '{}' does not exist.", dir_path);
    }

    let mut output = String::new();
    output.push_str(&format!("Directory listing for {}:\n", dir_path));

    match fs::read_dir(path) {
        Ok(entries) => {
            let mut items: Vec<String> = Vec::new();
            for entry in entries.flatten() {
                let p = entry.path();
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                let is_dir = p.is_dir();
                let suffix = if is_dir { "/" } else { "" };
                items.push(format!("{}{}", name, suffix));
            }
            items.sort();
            for item in items {
                output.push_str(&format!("{}\n", item));
            }
        }
        Err(e) => return format!("Error listing directory: {}", e),
    }
    output
}

pub fn read_file(file_path: &str, start_line: Option<usize>, end_line: Option<usize>) -> String {
    match fs::read_to_string(file_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();
            
            // If ranges are provided, slice the content
            if start_line.is_some() || end_line.is_some() {
                 let start = start_line.unwrap_or(1).saturating_sub(1); // 1-based to 0-based
                 let end = end_line.unwrap_or(total_lines).min(total_lines);
                 
                 if start >= total_lines {
                     return format!("Error: Start line {} is beyond file length ({} lines).", start + 1, total_lines);
                 }
                 
                 let selection = &lines[start..end];
                 let formatted_content = selection.join("\n");
                 
                 // Add truncation warning if applicable
                 let truncated_start = if start > 0 { format!("... ({} lines omitted)\n", start) } else { String::new() };
                 let truncated_end = if end < total_lines { format!("\n... ({} lines omitted)", total_lines - end) } else { String::new() };
                 
                 return format!("File: {} (Lines {}-{})\n{}{}{}", file_path, start + 1, end, truncated_start, formatted_content, truncated_end);
            }

            // Default: return full content (maybe truncate if huge?)
            // For now, let's return full content but warn about size if huge
            if content.len() > 40_000 {
                return format!("File: {} (Truncated - too large, use start_line/end_line to read specific parts)\n{}", file_path, &content[..40_000]);
            }
            format!("File: {}\n{}", file_path, content)
        }
        Err(e) => format!("Error reading file '{}': {}", file_path, e),
    }
}

pub fn write_file(file_path: &str, content: &str) -> String {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(file_path).parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return format!("Error creating parent directories for '{}': {}", file_path, e);
        }
    }

    match fs::write(file_path, content) {
        Ok(_) => format!("Successfully wrote to file: {}", file_path),
        Err(e) => format!("Error writing file '{}': {}", file_path, e),
    }
}

pub fn replace_text(file_path: &str, old_string: &str, new_string: &str) -> String {
    match fs::read_to_string(file_path) {
        Ok(content) => {
            let matches: Vec<_> = content.match_indices(old_string).collect();
            if matches.is_empty() {
                return format!("Error: 'old_string' not found in file '{}'. Ensure exact match (including whitespace).", file_path);
            }
            if matches.len() > 1 {
                 // Force unique matches for safety
                 return format!("Error: 'old_string' found {} times in file '{}'. Please provide more context to make it unique.", matches.len(), file_path);
            }
            let new_content = content.replace(old_string, new_string);
            match fs::write(file_path, new_content) {
                Ok(_) => format!("Successfully replaced text in file: {}", file_path),
                Err(e) => format!("Error writing file '{}': {}", file_path, e),
            }
        }
        Err(e) => format!("Error reading file '{}': {}", file_path, e),
    }
}

pub async fn run_shell_command(command: &str) -> String {
    // Detect OS and use appropriate shell
    let (shell, args) = if cfg!(target_os = "windows") {
        ("powershell", vec!["-NoProfile", "-Command", command])
    } else {
        ("sh", vec!["-c", command])
    };

    let cmd_result = timeout(Duration::from_secs(30), Command::new(shell).args(&args).output()).await;

    match cmd_result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut res = String::new();
            if !stdout.is_empty() {
                res.push_str(&format!("Output:\n{}\n", stdout));
            }
            if !stderr.is_empty() {
                res.push_str(&format!("Error:\n{}\n", stderr));
            }
            if res.is_empty() {
                res.push_str("(No output)");
            }
            res
        }
        Ok(Err(e)) => format!("Failed to execute command: {}", e),
        Err(_) => "Error: Command timed out after 30 seconds.".to_string(),
    }
}

pub async fn list_processes(sort_by: Option<&str>) -> String {
    let sort_col = match sort_by {
        Some(s) if s.to_lowercase().contains("mem") || s.to_lowercase().contains("ws") || s.to_lowercase().contains("rss") => {
            if cfg!(target_os = "windows") { "WorkingSet64" } else { "%mem" }
        },
        _ => if cfg!(target_os = "windows") { "CPU" } else { "%cpu" },
    };

    let command = if cfg!(target_os = "windows") {
        format!("Get-Process | Sort-Object {} -Descending | Select-Object -First 50 | Select-Object Name, @{{Name='CPU(s)';Expression={{$_.CPU}}}}, @{{Name='Memory(MB)';Expression={{[math]::round($_.WorkingSet64 / 1MB, 2)}}}}, Id", sort_col)
    } else {
        format!("ps aux --sort=-{} | head -n 50", sort_col)
    };
    run_shell_command(&command).await
}

pub async fn get_system_info() -> String {
    let os = if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" };
    
    let shell = if cfg!(target_os = "windows") {
        // Check if running in PowerShell
        let p_check = Command::new("powershell").args(&["-Command", "$PSVersionTable.PSVersion"]).output().await;
        if p_check.is_ok() { "PowerShell" } else { "CMD" }
    } else {
        "sh/bash"
    };

    format!("OS: {}\nShell: {}", os, shell)
}

pub fn grep_search(pattern: &str, dir_path: Option<&str>, include_pattern: Option<&str>) -> String {
    let search_dir = dir_path.unwrap_or(".");
    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return format!("Invalid regex pattern '{}': {}", pattern, e),
    };
    
    let glob_matcher = include_pattern.and_then(|p| glob::Pattern::new(p).ok());

    let mut matches = String::new();
    let mut match_count = 0;
    const MAX_MATCHES: usize = 100;

    // Use ignore crate to walk directory, respecting .gitignore
    let walker = WalkBuilder::new(search_dir)
        .hidden(false) // don't skip hidden files by default
        .git_ignore(true)
        .build();

    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            
            // Filter by glob if provided
            if let Some(ref matcher) = glob_matcher {
                if !matcher.matches(&path_str) {
                    continue;
                }
            }

            if let Ok(content) = fs::read_to_string(path) {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        matches.push_str(&format!("{}:{}: {}\n", path_str, i + 1, line.trim()));
                        match_count += 1;
                        if match_count >= MAX_MATCHES {
                            matches.push_str("... (Limit reached)\n");
                            return matches;
                        }
                    }
                }
            }
        }
    }

    if matches.is_empty() {
        return "No matches found.".to_string();
    }
    matches
}

pub fn ripgrep(pattern: &str) -> String {
    grep_search(pattern, None, None)
}


pub fn glob_search(pattern: &str) -> String {
    let mut results = String::new();
    match glob(pattern) {
        Ok(paths) => {
            for entry in paths {
                match entry {
                    Ok(path) => results.push_str(&format!("{}\n", path.display())),
                    Err(e) => results.push_str(&format!("Error reading glob entry: {}\n", e)),
                }
            }
        }
        Err(e) => return format!("Invalid glob pattern '{}': {}", pattern, e),
    }
    if results.is_empty() {
        return "No files matched the glob pattern.".to_string();
    }
    results
}

pub fn find_files(pattern: &str) -> String {
    let mut results = String::new();
    let walker = WalkDir::new(".").into_iter();
    let mut count = 0;
    
    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name.contains(pattern) {
                results.push_str(&format!("{}\n", entry.path().display()));
                count += 1;
            }
        }
        if count >= 50 {
            results.push_str("... (Limit reached)\n");
            break;
        }
    }
    
    if results.is_empty() {
        return "No files found matching the pattern.".to_string();
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_list_directory() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(file_path).unwrap();
        let sub_dir_path = dir.path().join("subdir");
        fs::create_dir(&sub_dir_path).unwrap();

        let output = list_directory(dir.path().to_str().unwrap());
        assert!(output.contains("test.txt"));
        assert!(output.contains("subdir/"));
    }

    #[test]
    fn test_read_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let content = "Line 1\nLine 2\nLine 3\nLine 4";
        fs::write(&file_path, content).unwrap();

        // Full read
        let output = read_file(file_path.to_str().unwrap(), None, None);
        assert!(output.contains(content));

        // Partial read
        let output_partial = read_file(file_path.to_str().unwrap(), Some(2), Some(3));
        assert!(output_partial.contains("Line 2"));
        assert!(output_partial.contains("Line 3"));
        assert!(!output_partial.contains("Line 1")); // Excluded
        assert!(!output_partial.contains("Line 4")); // Excluded
    }

    #[test]
    fn test_write_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");
        let content = "Hello, world!";

        let output = write_file(file_path.to_str().unwrap(), content);
        assert!(output.contains("Successfully wrote"));

        let read_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_replace_text() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("replace_me.txt");
        let content = "Hello OLD world!";
        fs::write(&file_path, content).unwrap();

        // Success case
        let output = replace_text(file_path.to_str().unwrap(), "OLD", "NEW");
        assert!(output.contains("Successfully replaced"));
        let new_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_content, "Hello NEW world!");

        // Fail case: Not found
        let output_fail = replace_text(file_path.to_str().unwrap(), "MISSING", "NEW");
        assert!(output_fail.contains("not found"));

        // Fail case: Multiple matches
        let file_path_multi = dir.path().join("multi.txt");
        fs::write(&file_path_multi, "foo foo").unwrap();
        let output_multi = replace_text(file_path_multi.to_str().unwrap(), "foo", "bar");
        assert!(output_multi.contains("found 2 times"));
    }

    #[test]
    fn test_grep_search() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("search.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"Hello\");\n}").unwrap();

        let output = grep_search("println!", Some(dir.path().to_str().unwrap()), None);
        assert!(output.contains("search.rs"));
        assert!(output.contains("println!(\"Hello\");"));

        let output_fail = grep_search("foobar", Some(dir.path().to_str().unwrap()), None);
        assert!(output_fail.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_run_shell_command() {
        let output = run_shell_command("echo HelloTest").await;
        if !output.contains("Error") {
             assert!(output.contains("HelloTest"));
        }
    }
}
