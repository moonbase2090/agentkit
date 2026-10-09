#[cfg(feature = "sqlite-sessions")]
#[test]
fn sqlite_session_replay_matches_jsonl() {
    use agentkit::{Event, SessionLog};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let jsonl_path = dir.path().join("session.jsonl");
    let sqlite_path = dir.path().join("session.db");

    let jsonl = SessionLog::open(&jsonl_path).unwrap();
    let sqlite = SessionLog::open_sqlite(&sqlite_path).unwrap();

    let event = Event::UserMessage {
        content: "hello".to_owned(),
    };
    jsonl.append(&event).unwrap();
    sqlite.append(&event).unwrap();

    let jsonl_events = jsonl.replay().unwrap();
    let sqlite_events = sqlite.replay().unwrap();
    assert_eq!(jsonl_events.len(), sqlite_events.len());
    assert!(matches!(
        (&jsonl_events[0], &sqlite_events[0]),
        (
            agentkit::Event::UserMessage { content: a },
            agentkit::Event::UserMessage { content: b }
        ) if a == b
    ));
}
