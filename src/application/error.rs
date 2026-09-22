use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("Error de conexión con la Dart VM: {0}")]
    ConnectionError(String),

    #[error("No hay ninguna aplicación Flutter conectada")]
    NotConnected,

    #[error("Elemento no encontrado para el finder: {0}")]
    WidgetNotFound(String),

    #[error("Error ejecutando comando de Flutter Driver: {0}")]
    DriverError(String),

    #[error("Timeout esperando estabilización de la UI (pumpAndSettle)")]
    PumpTimeout,

    #[error("Error de serialización/deserialización: {0}")]
    SerializationError(String),

    #[error("Error de sistema de archivos: {0}")]
    FileSystemError(String),

    #[error("No se encontró una función main() reconocible en '{0}' para inyectar Flutter Driver")]
    MainNotFound(String),

    #[error("Error interno del protocolo: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ApplicationError>;
