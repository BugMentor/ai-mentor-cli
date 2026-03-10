use ai_mentor_cli::mentor::MentorEvent;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_mentor_event_channel() {
    let (tx, mut rx) = mpsc::channel(10);
    
    tokio::spawn(async move {
        tx.send(MentorEvent::Chunk(1, "hello".to_string())).await.unwrap();
        tx.send(MentorEvent::Finished(1)).await.unwrap();
    });

    let event = rx.recv().await.unwrap();
    if let MentorEvent::Chunk(id, text) = event {
        assert_eq!(id, 1);
        assert_eq!(text, "hello");
    } else {
        panic!("Expected Chunk event");
    }

    let event = rx.recv().await.unwrap();
    if let MentorEvent::Finished(id) = event {
        assert_eq!(id, 1);
    } else {
        panic!("Expected Finished event");
    }
}
