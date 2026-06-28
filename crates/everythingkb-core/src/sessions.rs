use anyhow::Result;
use rusqlite::{params, Connection};

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_sessions (
                id TEXT PRIMARY KEY,
                history TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn get_history(&self, id: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT history FROM chat_sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(String::new())
        }
    }

    pub fn append_turn(&self, id: &str, user: &str, assistant: &str) -> Result<()> {
        let history = self.get_history(id)?;
        let new_history = format!(
            "{history}\nUser: {user}\nAssistant: {assistant}\n"
        );
        self.conn.execute(
            "INSERT INTO chat_sessions (id, history) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
               history = excluded.history,
               updated_at = strftime('%s','now')",
            params![id, new_history],
        )?;
        Ok(())
    }
}
