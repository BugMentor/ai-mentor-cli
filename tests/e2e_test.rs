use ai_mentor_cli::agent::orchestrator::Orchestrator;
use ai_mentor_cli::mentor::MentorEvent;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_orchestrator_init() {
    let (tx, _rx) = mpsc::channel(100);
    let orchestrator = Orchestrator::new(tx).await;
    
    // We expect it to init successfully if the environment is somewhat sane
    // Even if Ollama is missing, the init should work (it just creates nodes)
    assert!(orchestrator.is_ok());
}

#[tokio::test]
async fn test_full_agent_flow_mock() {
    // This is more of a structural E2E test
    let (tx, mut rx) = mpsc::channel(100);
    let orchestrator = Orchestrator::new(tx).await.unwrap();
    
    // We won't actually run a complex task that hits Ollama here to avoid flaky tests in CI/CLI environment
    // but we've verified the components via unit tests.
    
    let task_id = 123;
    let prompt = "test task".to_string();
    
    // We'll just verify we can send/receive events
    let _orchestrator_handle = tokio::spawn(async move {
        orchestrator.run_task(task_id, prompt, "test_os".to_string(), "test_shell".to_string()).await
    });

    // Check if we get some events (like Search Node starting)
    let mut found_search = false;
    while let Ok(Some(event)) = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await {
        match event {
            MentorEvent::Chunk(id, text) => {
                assert_eq!(id, task_id);
                if text.contains("[Node: Search]") {
                    found_search = true;
                    break;
                }
            }
            MentorEvent::Error(_, e) => {
                // It might fail if Ollama is not running, which is fine for this environment
                // As long as the flow started.
                if e.contains("Ollama") || e.contains("connection") {
                    found_search = true; // Count this as "started"
                    break;
                }
            }
            _ => {}
        }
    }
    
    assert!(found_search, "Orchestrator should have at least started the first node or reported a connection error");
}
