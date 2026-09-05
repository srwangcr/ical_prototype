use tokio::time::{interval, Duration, MissedTickBehavior};
use std::sync::Arc;
use crate::sync::SyncEngine;
use crate::config::Config;

pub struct Scheduler {
    engine: Arc<SyncEngine>,
    config: Config,
}

impl Scheduler {
    pub fn new(engine: Arc<SyncEngine>, config: Config) -> Self {
        // FIX #6: sincronizar cada 1 minuto contra Airbnb/Vrbo/Booking es
        // agresivo — esas plataformas rate-limitean o bloquean IPs que pegan
        // sus feeds ICS con esa frecuencia. Airbnb recomienda no bajar de
        // 15 minutos. Solo avisamos, no forzamos, por si tenés una razón para
        // un valor bajo en desarrollo/pruebas.
        if config.sync_interval_minutes < 15 {
            tracing::warn!(
                "⚠️ sync_interval_minutes = {} es muy agresivo para feeds ICS de terceros \
                 (Airbnb/Vrbo/Booking). Se recomienda 15 minutos o más para evitar rate-limits.",
                config.sync_interval_minutes
            );
        }

        Self { engine, config }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(
            self.config.sync_interval_minutes * 60
        ));
        // FIX #7: sin esto, si un sync tarda más que el intervalo, Tokio
        // dispara los ticks acumulados en ráfaga apenas el sync anterior
        // termina. Skip descarta los ticks perdidos en vez de acumularlos.
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        tracing::info!(
            "🔄 Scheduler iniciado. Intervalo: {} minutos",
            self.config.sync_interval_minutes
        );

        self.run_sync().await?;

        loop {
            interval.tick().await;
            if let Err(e) = self.run_sync().await {
                tracing::error!("Error en sincronización programada: {}", e);
            }
        }
    }

    async fn run_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("⏰ Ejecutando sincronización programada...");

        let channels: Vec<(String, String)> = self.config
            .channels
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.name.clone(), c.ics_url.clone()))
            .collect();

        if channels.is_empty() {
            tracing::warn!("⚠️ No hay canales habilitados para sincronizar");
            return Ok(());
        }

        let reports = self.engine.sync_all(&channels).await?;

        let total_conflicts: usize = reports.iter().map(|r| r.conflicts_found).sum();
        if total_conflicts > 0 {
            tracing::warn!("⚠️ Total de conflictos detectados en esta corrida: {}", total_conflicts);
        }

        // FIX #2: limpieza ahora basada en end_date (ver db.rs), no en
        // created_at, así que esto ya no borra reservas futuras por error.
        let deleted = self.engine.get_db().delete_old_reservations(90).await?;
        if deleted > 0 {
            tracing::info!("🗑️ {} reserva(s) vieja(s) eliminadas", deleted);
        }

        Ok(())
    }
}
