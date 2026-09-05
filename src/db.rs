use sqlx::sqlite::{SqlitePoolOptions, SqliteConnectOptions};
use sqlx::SqlitePool;
use chrono::{DateTime, Utc};
use std::str::FromStr;
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
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // FIX #1: SqlitePool::connect(path) espera una URL con esquema (sqlite://...),
        // no una ruta pelada. Usamos SqliteConnectOptions para que además cree el
        // archivo si no existe (create_if_missing) en lugar de fallar en el primer run.
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

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
        let record = sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM channels WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id,)) = record {
            // Si la URL cambió (p. ej. rotaron la key del ICS), la actualizamos.
            sqlx::query("UPDATE channels SET ics_url = ? WHERE id = ?")
                .bind(url)
                .bind(id)
                .execute(&self.pool)
                .await?;
            return Ok(id);
        }

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

    // FIX #4: antes se armaba el filtro de canal con format!() dentro del SQL.
    // Ahora usamos dos queries fijas y bindeamos siempre, sin concatenar valores.
    pub async fn check_overlaps(&self, start: i64, end: i64, exclude_channel: Option<i64>) -> Result<Vec<Reservation>> {
        let records = if let Some(channel_id) = exclude_channel {
            sqlx::query_as::<_, Reservation>(
                r#"
                SELECT id, channel_id, external_uid, start_date, end_date, summary, created_at
                FROM reservations
                WHERE start_date < ? AND end_date > ? AND channel_id != ?
                "#
            )
            .bind(end)
            .bind(start)
            .bind(channel_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Reservation>(
                r#"
                SELECT id, channel_id, external_uid, start_date, end_date, summary, created_at
                FROM reservations
                WHERE start_date < ? AND end_date > ?
                "#
            )
            .bind(end)
            .bind(start)
            .fetch_all(&self.pool)
            .await?
        };

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

    /// Devuelve los external_uid que ya tenemos guardados para un canal y que
    /// todavía no han terminado (reservas futuras o en curso). Se usa para
    /// reconciliar contra lo que trae el feed en la sincronización actual.
    pub async fn get_active_uids_for_channel(&self, channel_id: i64) -> Result<Vec<String>> {
        let now = Utc::now().timestamp();
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT external_uid FROM reservations WHERE channel_id = ? AND end_date >= ?"
        )
        .bind(channel_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(uid,)| uid).collect())
    }

    /// FIX #2 (la parte importante): borra reservas de un canal por external_uid.
    /// Se usa tanto para eventos marcados STATUS:CANCELLED como para reservas
    /// futuras que ya no aparecen en el feed (canceladas silenciosamente, que es
    /// lo más común en Airbnb/Vrbo: el evento simplemente desaparece del ICS).
    pub async fn delete_reservations_by_uids(&self, channel_id: i64, uids: &[String]) -> Result<u64> {
        if uids.is_empty() {
            return Ok(0);
        }

        let mut total = 0u64;
        for uid in uids {
            let result = sqlx::query(
                "DELETE FROM reservations WHERE channel_id = ? AND external_uid = ?"
            )
            .bind(channel_id)
            .bind(uid)
            .execute(&self.pool)
            .await?;
            total += result.rows_affected();
        }

        Ok(total)
    }

    /// FIX #2: antes se borraba por created_at (cuándo se insertó el registro),
    /// lo que borraba reservas futuras válidas y nunca limpiaba estadías viejas
    /// que se insertaron hace tiempo. Ahora se borra por end_date (cuándo termina
    /// la estadía), que es lo que realmente indica que una reserva ya es historia.
    pub async fn delete_old_reservations(&self, older_than_days: i64) -> Result<u64> {
        let cutoff = Utc::now().timestamp() - (older_than_days * 86400);
        let result = sqlx::query(
            "DELETE FROM reservations WHERE end_date < ?"
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as u64)
    }
}
