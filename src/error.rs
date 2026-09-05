use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum SyncError {
    #[error("Error de base de datos: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Error HTTP: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Error parseando ICS: {0}")]
    Ical(String),

    #[error("Error de configuración: {0}")]
    Config(String),

    #[error("Conflicto detectado: {0}")]
    Conflict(String),

    #[error("Error de timestamp: {0}")]
    Chrono(#[from] chrono::ParseError),

    #[error("Error de I/O: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SyncError>;
