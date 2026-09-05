use crate::db::{Database, Reservation as DbReservation};
use crate::parser::parse_ics;
use crate::error::Result;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashSet;
use std::time::Duration;

pub struct SyncEngine {
    db: Database,
    http_client: Client,
}

/// FIX #3: resultado de una sincronización. Antes un "conflicto detectado"
/// (el caso de uso normal de esta herramienta) se devolvía como Err, mezclado
/// con errores reales de red/parseo/DB. Ahora sync_channel siempre devuelve
/// Ok(SyncReport) salvo que algo realmente haya fallado, y el reporte trae
/// la cantidad de conflictos para que el caller decida cómo alertar.
#[derive(Debug, Default)]
pub struct SyncReport {
    pub channel_name: String,
    pub events_synced: usize,
    pub cancelled_removed: usize,
    pub conflicts_found: usize,
}

impl SyncEngine {
    pub fn new(db: Database) -> Self {
        // FIX #5: timeout explícito. Sin esto, un feed ICS colgado se lleva
        // por delante el scheduler entero.
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { db, http_client }
    }

    pub fn get_db(&self) -> &Database {
        &self.db
    }

    pub async fn sync_channel(&self, channel_name: &str, url: &str) -> Result<SyncReport> {
        tracing::info!("Sincronizando canal: {}", channel_name);

        let channel_id = self.db.get_or_create_channel(channel_name, url).await?;

        let response = self.http_client.get(url).send().await?;
        let content = response.text().await?;

        let events = parse_ics(&content)?;
        tracing::info!("Se encontraron {} eventos en {}", events.len(), channel_name);

        let mut report = SyncReport {
            channel_name: channel_name.to_string(),
            ..Default::default()
        };

        // FIX #2a: eventos con STATUS:CANCELLED explícito -> se borran si existían.
        let explicit_cancelled: Vec<String> = events
            .iter()
            .filter(|e| e.is_cancelled)
            .map(|e| e.uid.clone())
            .collect();

        let active_events: Vec<_> = events.into_iter().filter(|e| !e.is_cancelled).collect();
        let active_uids: HashSet<String> = active_events.iter().map(|e| e.uid.clone()).collect();

        // FIX #2b: la cancelación "silenciosa" es la más común en la práctica
        // (Airbnb/Vrbo simplemente dejan de listar el evento, no mandan
        // STATUS:CANCELLED). Reconciliamos: cualquier reserva futura que
        // teníamos guardada para este canal y que ya NO aparece en el feed
        // actual, se considera cancelada.
        let previously_active_uids = self.db.get_active_uids_for_channel(channel_id).await?;
        let silently_cancelled: Vec<String> = previously_active_uids
            .into_iter()
            .filter(|uid| !active_uids.contains(uid))
            .collect();

        let mut to_remove = explicit_cancelled;
        to_remove.extend(silently_cancelled);
        to_remove.sort();
        to_remove.dedup();

        if !to_remove.is_empty() {
            let removed = self.db.delete_reservations_by_uids(channel_id, &to_remove).await?;
            report.cancelled_removed = removed as usize;
            tracing::info!(
                "{} reserva(s) canceladas removidas en {}",
                removed, channel_name
            );
        }

        // Procesar eventos activos: guardar + chequear overlap.
        for event in active_events {
            let overlaps = self.db
                .check_overlaps(event.start_date, event.end_date, Some(channel_id))
                .await?;

            if !overlaps.is_empty() {
                let conflict_details = format!(
                    "Conflicto: {} en {} -> Overlap con {} reserva(s)",
                    event.uid, channel_name, overlaps.len()
                );
                tracing::warn!("{}", conflict_details);
                report.conflicts_found += 1;
                self.db.log_conflict(&conflict_details).await?;
            }

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
            report.events_synced += 1;
        }

        self.db.update_channel_sync_time(channel_id).await?;

        // Ya NO devolvemos Err por encontrar conflictos: encontrar un overbooking
        // es el trabajo de esta herramienta, no una falla del sistema. Los
        // conflictos ya quedaron registrados en sync_conflicts y en el reporte.
        Ok(report)
    }

    pub async fn sync_all(&self, channels: &[(String, String)]) -> Result<Vec<SyncReport>> {
        let mut results = Vec::new();

        for (name, url) in channels {
            match self.sync_channel(name, url).await {
                Ok(report) => {
                    if report.conflicts_found > 0 {
                        tracing::warn!(
                            "⚠️ {}: {} evento(s) sincronizados, {} conflicto(s), {} cancelación(es)",
                            name, report.events_synced, report.conflicts_found, report.cancelled_removed
                        );
                    } else {
                        tracing::info!(
                            "✅ {}: {} evento(s) sincronizados, {} cancelación(es)",
                            name, report.events_synced, report.cancelled_removed
                        );
                    }
                    results.push(report);
                }
                Err(e) => {
                    // Esto sí es un error real: falla de red, parseo o DB.
                    tracing::error!("❌ Error en {}: {}", name, e);
                }
            }
        }

        Ok(results)
    }
}
