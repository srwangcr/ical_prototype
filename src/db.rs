use sqlx::SqlitePool;
use chrono::{DateTime, Utc};
use crate::error::Result;

pub struct Database {
    pool: SqlitePool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Reservation {
    pub id: i64,
    pub channel_id: i64,
    pub external_uid: String,
    pub start_date: i64,
    pub end_date: i64,
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Database {
    pub async fn new(path: &str) -> Result<Self> {
        // Crear directorio si no existe
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let pool = SqlitePool::connect(path).await?;
        
        // Inicializar tablas
        sqlx::query(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            
            CREATE TABLE IF NOT EXISTS channels (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                ics_url TEXT NOT NULL,
                last_synced_at TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS reservations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel_id INTEGER NOT NULL,
                external_uid TEXT NOT NULL,
                start_date INTEGER NOT NULL,
                end_date INTEGER NOT NULL,
                summary TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
                UNIQUE(channel_id, external_uid)
            );

            CREATE INDEX IF NOT EXISTS idx_reservations_dates 
                ON reservations(start_date, end_date);
            CREATE INDEX IF NOT EXISTS idx_reservations_channel 
                ON reservations(channel_id);

            CREATE TABLE IF NOT EXISTS sync_conflicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conflict_timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                details TEXT NOT NULL,
                resolved INTEGER DEFAULT 0
            );
            "#
        )
        .execute(&pool)
        .await?;

        Ok(Database { pool })
    }

    pub async fn get_or_create_channel(&self, name: &str, url: &str) -> Result<i64> {
        // Primero intentar obtener el canal existente
        let record = sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM channels WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id,)) = record {
            return Ok(id);
        }

        // Si no existe, crearlo
        let result = sqlx::query_as::<_, (i64,)>(
            r#"
            INSERT INTO channels (name, ics_url) 
            VALUES (?, ?) 
            RETURNING id
            "#
        )
        .bind(name)
        .bind(url)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    pub async fn update_channel_sync_time(&self, channel_id: i64) -> Result<()> {
        sqlx::query(
            "UPDATE channels SET last_synced_at = CURRENT_TIMESTAMP WHERE id = ?"
        )
        .bind(channel_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_reservation(&self, reservation: &Reservation) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO reservations 
            (channel_id, external_uid, start_date, end_date, summary)
            VALUES (?, ?, ?, ?, ?)
            "#
        )
        .bind(reservation.channel_id)
        .bind(&reservation.external_uid)
        .bind(reservation.start_date)
        .bind(reservation.end_date)
        .bind(&reservation.summary)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn check_overlaps(&self, start: i64, end: i64, exclude_channel: Option<i64>) -> Result<Vec<Reservation>> {
        let mut query = String::from(
            r#"
            SELECT id, channel_id, external_uid, start_date, end_date, summary, created_at
            FROM reservations 
            WHERE start_date < ? AND end_date > ?
            "#
        );

        if let Some(channel_id) = exclude_channel {
            query.push_str(&format!(" AND channel_id != {}", channel_id));
        }

        let records = sqlx::query_as::<_, Reservation>(&query)
            .bind(end)
            .bind(start)
            .fetch_all(&self.pool)
            .await?;

        Ok(records)
    }

    pub async fn log_conflict(&self, details: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_conflicts (details) VALUES (?)"
        )
        .bind(details)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_old_reservations(&self, older_than_days: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM reservations WHERE created_at < datetime('now', ?)"
        )
        .bind(format!("-{} days", older_than_days))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as u64)
    }
}