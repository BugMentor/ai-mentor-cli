use ai_mentor_cli::agent::executor::sanitize_tool_args;

#[test]
fn test_sanitize_tool_args_robustness() {
    // Basic prefix stripping
    assert_eq!(sanitize_tool_args("run_shell_command", "run_shell_command \"powershell ...\""), "powershell ...");
    assert_eq!(sanitize_tool_args("run_shell_command", "run_shell_command 'powershell ...'"), "powershell ...");
    
    // Parentheses stripping
    assert_eq!(sanitize_tool_args("run_shell_command", "run_shell_command(ls -la)"), "ls -la");
    assert_eq!(sanitize_tool_args("list_directory", "list_directory(./src)"), "./src");

    // Double quote stripping (model often adds quotes around the whole thing)
    assert_eq!(sanitize_tool_args("run_shell_command", "\"run_shell_command ls\""), "ls");
    assert_eq!(sanitize_tool_args("run_shell_command", "'run_shell_command ls'"), "ls");

    // No stripping needed
    assert_eq!(sanitize_tool_args("run_shell_command", "ls -la"), "ls -la");

    // Multiple spaces
    assert_eq!(sanitize_tool_args("run_shell_command", "run_shell_command     dir"), "dir");

    // Mixed cases
    assert_eq!(sanitize_tool_args("run_shell_command", "RUN_SHELL_COMMAND ls"), "RUN_SHELL_COMMAND ls"); // We keep it if case doesn't match exactly for safety, but usually model uses lowercase
}

#[test]
fn test_sanitize_tool_args_complex_powershell() {
    let input = "run_shell_command \"powershell (Get-Process | Sort-Object WS -Descending)\"";
    assert_eq!(sanitize_tool_args("run_shell_command", input), "powershell (Get-Process | Sort-Object WS -Descending)");
}
