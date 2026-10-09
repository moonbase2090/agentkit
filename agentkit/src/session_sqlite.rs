use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::session::Event;

pub struct SqliteSession {
    conn: Connection,
}

impl SqliteSession {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("open sqlite {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn append(&self, event: &Event) -> Result<()> {
        let payload = serde_json::to_string(event)?;
        self.conn
            .execute("INSERT INTO events (payload) VALUES (?1)", params![payload])
            .context("insert session event")?;
        Ok(())
    }

    pub fn replay(&self) -> Result<Vec<Event>> {
        let mut statement = self
            .conn
            .prepare("SELECT payload FROM events ORDER BY id ASC")
            .context("prepare session replay")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .context("query session events")?;
        let mut events = Vec::new();
        for row in rows {
            let payload = row.context("read session row")?;
            let event: Event = serde_json::from_str(&payload).context("parse session event")?;
            events.push(event);
        }
        Ok(events)
    }
}
