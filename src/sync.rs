use crate::db::{Database, Reservation as DbReservation};
use crate::parser::parse_ics;
use crate::error::{Result, SyncError};
use chrono::Utc;
use reqwest::Client;

pub struct SyncEngine {
    db: Database,
    http_client: Client,
}

impl SyncEngine {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            http_client: Client::new(),
        }
    }

    pub fn get_db(&self) -> &Database {
        &self.db
    }

    pub async fn sync_channel(&self, channel_name: &str, url: &str) -> Result<()> {
        tracing::info!("Sincronizando canal: {}", channel_name);

        // 1. Obtener o crear canal
        let channel_id = self.db.get_or_create_channel(channel_name, url).await?;

        // 2. Descargar el archivo ICS
        let response = self.http_client.get(url).send().await?;
        let content = response.text().await?;

        // 3. Parsear eventos
        let events = parse_ics(&content)?;
        tracing::info!("Se encontraron {} eventos en {}", events.len(), channel_name);

        // 4. Procesar cada evento
        let mut conflicts = Vec::new();
        for event in events {
            // Verificar si hay overlapping con otros canales
            let overlaps = self.db
                .check_overlaps(event.start_date, event.end_date, Some(channel_id))
                .await?;

            if !overlaps.is_empty() {
                let conflict_details = format!(
                    "Conflicto: {} en {} -> Overlap con {} reserva(s)",
                    event.uid, channel_name, overlaps.len()
                );
                tracing::warn!("{}", conflict_details);
                conflicts.push((event.clone(), overlaps));
                self.db.log_conflict(&conflict_details).await?;
            }

            // Guardar reserva (incluso si hay conflicto, registrarla para auditoría)
            let reservation = DbReservation {
                id: 0,
                channel_id,
                external_uid: event.uid,
                start_date: event.start_date,
                end_date: event.end_date,
                summary: event.summary,
                created_at: Utc::now(),
            };
            self.db.insert_reservation(&reservation).await?;
        }

        // 5. Actualizar timestamp de sincronización
        self.db.update_channel_sync_time(channel_id).await?;

        if !conflicts.is_empty() {
            return Err(SyncError::Conflict(format!(
                "Se encontraron {} conflictos en {}",
                conflicts.len(),
                channel_name
            )));
        }

        Ok(())
    }

    pub async fn sync_all(&self, channels: &[(String, String)]) -> Result<Vec<String>> {
        let mut results = Vec::new();

        for (name, url) in channels {
            match self.sync_channel(name, url).await {
                Ok(_) => {
                    results.push(format!("✅ {} sincronizado correctamente", name));
                }
                Err(e) => {
                    let msg = format!("❌ Error en {}: {}", name, e);
                    tracing::error!("{}", msg);
                    results.push(msg);
                }
            }
        }

        Ok(results)
    }
}