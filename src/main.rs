mod config;
mod db;
mod error;
mod parser;
mod sync;
mod scheduler;

use std::sync::Arc;
use tracing_subscriber::filter::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializar logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sync_engine=info"));
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    tracing::info!("🚀 Iniciando Sync Engine...");

    // Crear directorio para la base de datos
    let db_path = "sync_engine.db";
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Cargar configuración
    let config = config::Config::load("config.toml")?;
    tracing::info!("📋 Configuración cargada");

    // Inicializar base de datos
    let db = db::Database::new(db_path).await?;
    tracing::info!("💾 Base de datos conectada");

    // Crear motor de sincronización
    let sync_engine = Arc::new(sync::SyncEngine::new(db));
    tracing::info!("⚙️ Motor de sincronización creado");

    // Iniciar scheduler
    let scheduler = scheduler::Scheduler::new(sync_engine, config);
    
    if let Err(e) = scheduler.start().await {
        tracing::error!("❌ Error en scheduler: {}", e);
        return Err(e);
    }

    Ok(())
}