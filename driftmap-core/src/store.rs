use rusqlite::{Connection, params};
use crate::state::DriftState;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS endpoint_state (
                endpoint    TEXT PRIMARY KEY,
                state       TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS drift_scores (
                endpoint    TEXT NOT NULL,
                score       REAL NOT NULL,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_drift_scores_endpoint
                ON drift_scores(endpoint, recorded_at DESC);

            CREATE TABLE IF NOT EXISTS diverging_pairs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                endpoint    TEXT NOT NULL,
                req_method  TEXT,
                req_path    TEXT,
                status_a    INTEGER,
                status_b    INTEGER,
                body_a      BLOB,
                body_b      BLOB,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pairs_endpoint
                ON diverging_pairs(endpoint, recorded_at DESC);
        ")?;
        Ok(Self { conn })
    }

    pub fn save_state(&self, endpoint: &str, state: &DriftState) -> anyhow::Result<()> {
        let state_str = match state {
            DriftState::Unknown => "Unknown",
            DriftState::Equivalent => "Equivalent",
            DriftState::Drifting => "Drifting",
            DriftState::Diverged => "Diverged",
        };
        
        self.conn.execute(
            "INSERT OR REPLACE INTO endpoint_state (endpoint, state, updated_at)
             VALUES (?1, ?2, strftime('%s','now'))",
            params![endpoint, state_str],
        )?;
        Ok(())
    }

    pub fn save_score(&self, endpoint: &str, score: f32) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO drift_scores (endpoint, score, recorded_at)
             VALUES (?1, ?2, strftime('%s','now'))",
            params![endpoint, score],
        )?;
        
        // Prune old scores (keep last 24h)
        self.conn.execute(
            "DELETE FROM drift_scores
             WHERE endpoint = ?1
               AND recorded_at < strftime('%s','now') - 86400",
            params![endpoint],
        )?;
        Ok(())
    }

    pub fn recent_scores(&self, endpoint: &str, limit: usize) -> anyhow::Result<Vec<(i64, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT recorded_at, score FROM drift_scores
             WHERE endpoint = ?1
             ORDER BY recorded_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![endpoint, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
