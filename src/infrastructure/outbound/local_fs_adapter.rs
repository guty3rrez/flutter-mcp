use async_trait::async_trait;

use crate::application::error::{ApplicationError, Result};
use crate::application::ports::outbound::ProjectFilesPort;

/// Adaptador secundario que lee/escribe archivos del proyecto Flutter directamente en el disco
/// local (`flutter-mcp` corre en la misma máquina que `flutter run`). Usado por
/// `flutter_start_control` para inyectar Flutter Driver en el entrypoint de una app conectada.
pub struct LocalFileSystemAdapter;

impl LocalFileSystemAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalFileSystemAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProjectFilesPort for LocalFileSystemAdapter {
    async fn read_to_string(&self, path: &str) -> Result<String> {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ApplicationError::FileSystemError(format!("Leyendo '{path}': {e}")))
    }

    async fn write_string(&self, path: &str, content: &str) -> Result<()> {
        tokio::fs::write(path, content)
            .await
            .map_err(|e| ApplicationError::FileSystemError(format!("Escribiendo '{path}': {e}")))
    }
}
