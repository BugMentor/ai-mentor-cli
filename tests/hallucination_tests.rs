use ai_mentor_cli::agent::executor::is_hallucination;
use ai_mentor_cli::agent::planner::parse_steps;

#[test]
fn test_hallucination_detection_integration() {
    // Test exact trigger strings
    assert!(is_hallucination("Question: Let w(q) = 2*q3 + 10*q2 - 5*q + 4."));
    assert!(is_hallucination("Let y be w(-6)."));
    assert!(is_hallucination("Suppose 0 = -y*h + 7*h - 8."));
    assert!(is_hallucination("Solve h*x + 2*x = 0 for x."));
    // Test case independence
    assert!(is_hallucination("question: let w(q)"));
    assert!(is_hallucination("I WILL USE run_shell_command"));
    assert!(is_hallucination("CALLING list_processes..."));
    
    // Test non-hallucinations
    assert!(!is_hallucination("The current memory usage is 45%."));
    assert!(!is_hallucination("Run cargo build --release"));
}

#[test]
fn test_planner_parsing_integration() {
    let output = r#"
Plan for listing processes:
1. Run Get-Process in PowerShell
2. Sort by memory consumption (WorkingSet64)
3. Return the top 10 results
"#;
    let steps = parse_steps(output);
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0], "Run Get-Process in PowerShell");
    assert_eq!(steps[1], "Sort by memory consumption (WorkingSet64)");
    assert_eq!(steps[2], "Return the top 10 results");
}

#[test]
fn test_prefix_stripping_robustness() {
    assert_eq!(parse_steps("1. Step A")[0], "Step A");
    assert_eq!(parse_steps("- Step B")[0], "Step B");
    assert_eq!(parse_steps("* Step C")[0], "Step C");
    assert_eq!(parse_steps("   1.   Step D")[0], "Step D");
}
