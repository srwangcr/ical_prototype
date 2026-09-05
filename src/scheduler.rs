use tokio::time::{interval, Duration};
use std::sync::Arc;
use crate::sync::SyncEngine;
use crate::config::Config;

pub struct Scheduler {
    engine: Arc<SyncEngine>,
    config: Config,
}

impl Scheduler {
    pub fn new(engine: Arc<SyncEngine>, config: Config) -> Self {
        Self { engine, config }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(
            self.config.sync_interval_minutes * 60
        ));

        tracing::info!(
            "🔄 Scheduler iniciado. Intervalo: {} minutos",
            self.config.sync_interval_minutes
        );

        // Primera sincronización inmediata
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

        let results = self.engine.sync_all(&channels).await?;
        
        for result in &results {
            tracing::info!("{}", result);
        }

        // Limpiar reservas viejas (más de 90 días)
        let deleted = self.engine.get_db().delete_old_reservations(90).await?;
        if deleted > 0 {
            tracing::info!("🗑️ {} reservas viejas eliminadas", deleted);
        }

        Ok(())
    }
}