use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Pending,
    Indexed,
    Failed,
    Skipped,
}

impl FileStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Indexed => "indexed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "indexed" => Self::Indexed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub file_hash: String,
    pub mtime: i64,
    pub size: i64,
    pub status: FileStatus,
    pub doc_name: Option<String>,
    pub error: Option<String>,
}

pub struct Registry {
    conn: Connection,
}

impl Registry {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open registry {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                file_hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                size INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                doc_name TEXT,
                error TEXT,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            CREATE INDEX IF NOT EXISTS idx_files_hash ON files(file_hash);
            CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);",
        )?;
        Ok(Self { conn })
    }

    pub fn hash_file(path: &Path) -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn get(&self, path: &str) -> Result<Option<FileRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, file_hash, mtime, size, status, doc_name, error FROM files WHERE path = ?1",
        )?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(FileRecord {
                path: row.get(0)?,
                file_hash: row.get(1)?,
                mtime: row.get(2)?,
                size: row.get(3)?,
                status: FileStatus::from_str(row.get::<_, String>(4)?.as_str()),
                doc_name: row.get(5)?,
                error: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn needs_reindex(&self, path: &Path, mtime: i64, size: i64, hash: &str) -> Result<bool> {
        let key = path.to_string_lossy();
        match self.get(&key)? {
            None => Ok(true),
            Some(rec) if rec.file_hash != hash || rec.mtime != mtime || rec.size != size => Ok(true),
            Some(rec) if rec.status == FileStatus::Failed => Ok(true),
            Some(rec) if rec.status == FileStatus::Pending => Ok(true),
            Some(_) => Ok(false),
        }
    }

    pub fn upsert(
        &self,
        path: &str,
        file_hash: &str,
        mtime: i64,
        size: i64,
        status: FileStatus,
        doc_name: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO files (path, file_hash, mtime, size, status, doc_name, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
               file_hash = excluded.file_hash,
               mtime = excluded.mtime,
               size = excluded.size,
               status = excluded.status,
               doc_name = excluded.doc_name,
               error = excluded.error,
               updated_at = strftime('%s','now')",
            params![
                path,
                file_hash,
                mtime,
                size,
                status.as_str(),
                doc_name,
                error
            ],
        )?;
        Ok(())
    }

    pub fn stats(&self) -> Result<RegistryStats> {
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let indexed: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'indexed'",
            [],
            |r| r.get(0),
        )?;
        let failed: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'failed'",
            [],
            |r| r.get(0),
        )?;
        let pending: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )?;
        Ok(RegistryStats {
            total,
            indexed,
            failed,
            pending,
        })
    }

    pub fn list_indexed(&self) -> Result<Vec<FileRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, file_hash, mtime, size, status, doc_name, error FROM files
             WHERE status = 'indexed' ORDER BY path",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                file_hash: row.get(1)?,
                mtime: row.get(2)?,
                size: row.get(3)?,
                status: FileStatus::from_str(row.get::<_, String>(4)?.as_str()),
                doc_name: row.get(5)?,
                error: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total: i64,
    pub indexed: i64,
    pub failed: i64,
    pub pending: i64,
}

pub fn file_metadata(path: &Path) -> Result<(i64, i64)> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok((mtime, meta.len() as i64))
}

pub fn portable_path(path: &Path, kb_root: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let kb = kb_root.canonicalize().unwrap_or_else(|_| kb_root.to_path_buf());
    if resolved.starts_with(&kb) {
        resolved
            .strip_prefix(&kb)
            .unwrap_or(&resolved)
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string()
    } else {
        resolved.to_string_lossy().into_owned()
    }
}
