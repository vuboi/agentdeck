//! Append-only SQLite event store with a SHA-256 hash chain.
//!
//! Each row stores the canonical JSON of its `EventKind` as text. The hash covers `seq`,
//! `prev_hash` and every other column, so editing, reordering, renumbering or deleting
//! a row breaks `verify_chain`. Triggers block UPDATE and DELETE at the database level.
//!
//! Known limit: deleting the newest rows leaves a valid shorter chain. Detect that by saving
//! `head()` outside the database and checking it later with `verify_chain_to_head`.

use std::{
    io::Write,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ag_protocol::{Event, EventKind};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
    Row, Sqlite, SqlitePool, Transaction,
};
use thiserror::Error;

/// `prev_hash` of the first event in the chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Bump when the table layout changes. Stored in `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("hash chain broken at seq {seq}")]
    ChainBroken { seq: i64 },
    #[error("chain head mismatch: expected seq {expected_seq}, found seq {found_seq}")]
    HeadMismatch { expected_seq: i64, found_seq: i64 },
    #[error("database schema version {found} is newer than supported {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
}

pub type Result<T> = std::result::Result<T, StoreError>;

const SCHEMA: [&str; 5] = [
    r#"CREATE TABLE IF NOT EXISTS events (
        seq INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        session_id TEXT NOT NULL,
        turn_id TEXT,
        ts INTEGER NOT NULL,
        kind_json TEXT NOT NULL,
        prev_hash TEXT NOT NULL UNIQUE,
        hash TEXT NOT NULL
    )"#,
    "CREATE INDEX IF NOT EXISTS idx_events_session ON events (session_id, seq)",
    "CREATE INDEX IF NOT EXISTS idx_events_turn ON events (turn_id, seq)",
    r#"CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END"#,
    r#"CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END"#,
];

const SELECT_ALL: &str = "SELECT seq, id, session_id, turn_id, ts, kind_json, prev_hash, hash
     FROM events ORDER BY seq";
const SELECT_SESSION: &str = "SELECT seq, id, session_id, turn_id, ts, kind_json, prev_hash, hash
     FROM events WHERE session_id = ? AND seq > ? ORDER BY seq LIMIT ?";
const SELECT_TURN: &str = "SELECT seq, id, session_id, turn_id, ts, kind_json, prev_hash, hash
     FROM events WHERE turn_id = ? ORDER BY seq";

/// Handle to the event database. Cheap to clone.
#[derive(Clone)]
pub struct EventStore {
    pool: SqlitePool,
}

impl EventStore {
    /// Opens (or creates) a database file and applies the schema.
    pub async fn open(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Self::with_pool(pool).await
    }

    /// In-memory database for tests. A single connection keeps the data alive.
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await?;
        Self::with_pool(pool).await
    }

    async fn with_pool(pool: SqlitePool) -> Result<Self> {
        let found: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await?;
        if found > SCHEMA_VERSION {
            return Err(StoreError::SchemaTooNew {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        if found < SCHEMA_VERSION {
            for statement in SCHEMA {
                sqlx::query(statement).execute(&pool).await?;
            }
            sqlx::query("PRAGMA user_version = 1")
                .execute(&pool)
                .await?;
        }
        Ok(Self { pool })
    }

    /// Appends one event and returns it with `seq`, `id`, `ts` and hashes filled in.
    ///
    /// Runs under `BEGIN IMMEDIATE`, so concurrent writers queue instead of forking the chain.
    /// The unique index on `prev_hash` is a second guard.
    pub async fn append(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        kind: EventKind,
    ) -> Result<Event> {
        let kind_json = serde_json::to_string(&kind)?;
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now_millis();

        let mut tx: Transaction<'_, Sqlite> = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let tail: Option<(i64, String)> =
            sqlx::query_as("SELECT seq, hash FROM events ORDER BY seq DESC LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?;
        let (seq, prev_hash) = match tail {
            Some((last_seq, last_hash)) => (last_seq + 1, last_hash),
            None => (1, GENESIS_HASH.to_owned()),
        };
        let hash = compute_hash(seq, &prev_hash, &id, session_id, turn_id, ts, &kind_json);

        sqlx::query(
            "INSERT INTO events (seq, id, session_id, turn_id, ts, kind_json, prev_hash, hash)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(seq)
        .bind(&id)
        .bind(session_id)
        .bind(turn_id)
        .bind(ts)
        .bind(&kind_json)
        .bind(&prev_hash)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(Event {
            seq,
            id,
            session_id: session_id.to_owned(),
            turn_id: turn_id.map(str::to_owned),
            ts,
            kind,
            prev_hash,
            hash,
        })
    }

    /// Events of one session with `seq > after_seq`, oldest first.
    pub async fn list_session(
        &self,
        session_id: &str,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<Event>> {
        let rows = sqlx::query(SELECT_SESSION)
            .bind(session_id)
            .bind(after_seq)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_event).collect()
    }

    /// All events of one turn, oldest first.
    pub async fn list_turn(&self, turn_id: &str) -> Result<Vec<Event>> {
        let rows = sqlx::query(SELECT_TURN)
            .bind(turn_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_event).collect()
    }

    /// Latest `(seq, hash)` of the chain, or `None` when the store is empty.
    pub async fn head(&self) -> Result<Option<(i64, String)>> {
        let head = sqlx::query_as("SELECT seq, hash FROM events ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(head)
    }

    /// Re-computes every hash from the first event. Checks `seq` runs 1..n with no gaps.
    pub async fn verify_chain(&self) -> Result<()> {
        let rows = sqlx::query(SELECT_ALL).fetch_all(&self.pool).await?;

        let mut expected_prev = GENESIS_HASH.to_owned();
        for (index, row) in rows.iter().enumerate() {
            let expected_seq = index as i64 + 1;
            let seq: i64 = row.try_get("seq")?;
            let id: String = row.try_get("id")?;
            let session_id: String = row.try_get("session_id")?;
            let turn_id: Option<String> = row.try_get("turn_id")?;
            let ts: i64 = row.try_get("ts")?;
            let kind_json: String = row.try_get("kind_json")?;
            let prev_hash: String = row.try_get("prev_hash")?;
            let stored_hash: String = row.try_get("hash")?;

            let recomputed = compute_hash(
                seq,
                &prev_hash,
                &id,
                &session_id,
                turn_id.as_deref(),
                ts,
                &kind_json,
            );
            if seq != expected_seq || prev_hash != expected_prev || recomputed != stored_hash {
                return Err(StoreError::ChainBroken { seq });
            }
            expected_prev = stored_hash;
        }
        Ok(())
    }

    /// `verify_chain`, then checks the chain still ends at the saved head. Catches tail truncation.
    pub async fn verify_chain_to_head(&self, saved: &(i64, String)) -> Result<()> {
        self.verify_chain().await?;
        let found = self.head().await?;
        if found.as_ref() == Some(saved) {
            return Ok(());
        }
        Err(StoreError::HeadMismatch {
            expected_seq: saved.0,
            found_seq: found.map_or(0, |(seq, _)| seq),
        })
    }

    /// Writes every event as one JSON object per line. Returns the number of lines written.
    pub async fn export_jsonl<W: Write>(&self, writer: &mut W) -> Result<usize> {
        let rows = sqlx::query(SELECT_ALL).fetch_all(&self.pool).await?;
        for row in &rows {
            serde_json::to_writer(&mut *writer, &row_to_event(row)?)?;
            writer.write_all(b"\n")?;
        }
        Ok(rows.len())
    }
}

/// Every field is length-prefixed, and `turn_id` has a presence byte. That keeps `None`
/// distinct from `Some("")` and removes ambiguity at field boundaries.
fn compute_hash(
    seq: i64,
    prev_hash: &str,
    id: &str,
    session_id: &str,
    turn_id: Option<&str>,
    ts: i64,
    kind_json: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seq.to_be_bytes());
    put(&mut hasher, prev_hash.as_bytes());
    put(&mut hasher, id.as_bytes());
    put(&mut hasher, session_id.as_bytes());
    match turn_id {
        Some(turn) => {
            hasher.update([1u8]);
            put(&mut hasher, turn.as_bytes());
        }
        None => hasher.update([0u8]),
    }
    hasher.update(ts.to_be_bytes());
    put(&mut hasher, kind_json.as_bytes());
    hex::encode(hasher.finalize())
}

fn put(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn row_to_event(row: &SqliteRow) -> Result<Event> {
    let kind_json: String = row.try_get("kind_json")?;
    Ok(Event {
        seq: row.try_get("seq")?,
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        ts: row.try_get("ts")?,
        kind: serde_json::from_str(&kind_json)?,
        prev_hash: row.try_get("prev_hash")?,
        hash: row.try_get("hash")?,
    })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> EventKind {
        EventKind::MessageDelta { text: s.into() }
    }

    fn temp_db_path() -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("ag-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.db").to_string_lossy().replace('\\', "/");
        (dir, path)
    }

    /// Drops the update guard, then rewrites one column of one row.
    async fn tamper(store: &EventStore, seq: i64, column: &str, value: &str) {
        sqlx::query("DROP TRIGGER events_no_update")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE events SET {column} = ? WHERE seq = ?"
        )))
        .bind(value)
        .bind(seq)
        .execute(&store.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn empty_store_verifies_and_has_no_head() {
        let store = EventStore::in_memory().await.unwrap();
        store.verify_chain().await.unwrap();
        assert!(store.head().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_then_query_by_session_and_turn() {
        let store = EventStore::in_memory().await.unwrap();
        let a = store.append("s1", Some("t1"), text("hello")).await.unwrap();
        let other = store.append("s2", None, text("other")).await.unwrap();
        let b = store.append("s1", Some("t1"), text("world")).await.unwrap();

        assert_eq!(a.seq, 1);
        assert_eq!(a.prev_hash, GENESIS_HASH);
        assert_eq!(other.prev_hash, a.hash);
        assert_eq!(b.prev_hash, other.hash);

        let events = store.list_session("s1", 0, 100).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, text("hello"));
        assert_eq!(events[1].seq, b.seq);
        assert_eq!(store.list_session("s1", a.seq, 100).await.unwrap().len(), 1);

        assert_eq!(store.list_turn("t1").await.unwrap().len(), 2);
        assert!(store.list_turn("missing").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn verify_passes_on_untouched_chain() {
        let store = EventStore::in_memory().await.unwrap();
        for i in 0..5 {
            store
                .append("s1", None, text(&i.to_string()))
                .await
                .unwrap();
        }
        store.verify_chain().await.unwrap();
    }

    #[tokio::test]
    async fn update_and_delete_are_blocked_by_triggers() {
        let store = EventStore::in_memory().await.unwrap();
        store.append("s1", None, text("x")).await.unwrap();
        assert!(sqlx::query("UPDATE events SET kind_json = '{}'")
            .execute(&store.pool)
            .await
            .is_err());
        assert!(sqlx::query("DELETE FROM events")
            .execute(&store.pool)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn detects_tampering_of_every_column() {
        let cases: [(&str, &str); 6] = [
            ("kind_json", r#"{"type":"message_delta","text":"forged"}"#),
            ("prev_hash", "ffff"),
            ("id", "forged-id"),
            ("session_id", "forged-session"),
            ("turn_id", "forged-turn"),
            ("ts", "1"),
        ];
        for (column, value) in cases {
            let store = EventStore::in_memory().await.unwrap();
            store.append("s1", Some("t1"), text("one")).await.unwrap();
            let target = store.append("s1", Some("t1"), text("two")).await.unwrap();
            store.append("s1", Some("t1"), text("three")).await.unwrap();

            tamper(&store, target.seq, column, value).await;

            match store.verify_chain().await {
                Err(StoreError::ChainBroken { seq }) => {
                    assert_eq!(seq, target.seq, "column {column}")
                }
                other => panic!("column {column}: expected ChainBroken, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn detects_renumbered_seq() {
        let store = EventStore::in_memory().await.unwrap();
        store.append("s1", None, text("one")).await.unwrap();
        let second = store.append("s1", None, text("two")).await.unwrap();
        store.append("s1", None, text("three")).await.unwrap();

        tamper(&store, second.seq, "seq", "99").await;
        assert!(matches!(
            store.verify_chain().await,
            Err(StoreError::ChainBroken { .. })
        ));
    }

    #[tokio::test]
    async fn detects_deleted_middle_row() {
        let store = EventStore::in_memory().await.unwrap();
        store.append("s1", None, text("one")).await.unwrap();
        let middle = store.append("s1", None, text("two")).await.unwrap();
        store.append("s1", None, text("three")).await.unwrap();

        sqlx::query("DROP TRIGGER events_no_delete")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM events WHERE seq = ?")
            .bind(middle.seq)
            .execute(&store.pool)
            .await
            .unwrap();

        assert!(matches!(
            store.verify_chain().await,
            Err(StoreError::ChainBroken { .. })
        ));
    }

    #[tokio::test]
    async fn tail_truncation_is_caught_by_saved_head() {
        let store = EventStore::in_memory().await.unwrap();
        store.append("s1", None, text("one")).await.unwrap();
        store.append("s1", None, text("two")).await.unwrap();
        let saved = store.head().await.unwrap().unwrap();
        store.verify_chain_to_head(&saved).await.unwrap();

        sqlx::query("DROP TRIGGER events_no_delete")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM events WHERE seq = ?")
            .bind(saved.0)
            .execute(&store.pool)
            .await
            .unwrap();

        // The shorter chain is internally valid, so only the saved head exposes the cut.
        store.verify_chain().await.unwrap();
        assert!(matches!(
            store.verify_chain_to_head(&saved).await,
            Err(StoreError::HeadMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn concurrent_appends_keep_one_valid_chain() {
        let (dir, path) = temp_db_path();
        let store = EventStore::open(&path).await.unwrap();

        let mut tasks = Vec::new();
        for i in 0..20 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .append(&format!("s{}", i % 3), None, text(&i.to_string()))
                    .await
            }));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }

        store.verify_chain().await.unwrap();
        assert_eq!(store.head().await.unwrap().unwrap().0, 20);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn export_writes_one_json_line_per_event() {
        let store = EventStore::in_memory().await.unwrap();
        store.append("s1", None, text("a")).await.unwrap();
        store.append("s1", None, text("b")).await.unwrap();

        let mut buf: Vec<u8> = Vec::new();
        let written = store.export_jsonl(&mut buf).await.unwrap();
        assert_eq!(written, 2);
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out.lines().count(), 2);
        for line in out.lines() {
            let _: Event = serde_json::from_str(line).unwrap();
        }
    }

    #[tokio::test]
    async fn file_database_persists_and_rejects_newer_schema() {
        let (dir, path) = temp_db_path();
        {
            let store = EventStore::open(&path).await.unwrap();
            store.append("s1", None, text("kept")).await.unwrap();
        }
        let store = EventStore::open(&path).await.unwrap();
        assert_eq!(store.list_session("s1", 0, 10).await.unwrap().len(), 1);
        store.verify_chain().await.unwrap();

        sqlx::query("PRAGMA user_version = 99")
            .execute(&store.pool)
            .await
            .unwrap();
        drop(store);
        assert!(matches!(
            EventStore::open(&path).await,
            Err(StoreError::SchemaTooNew { found: 99, .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
