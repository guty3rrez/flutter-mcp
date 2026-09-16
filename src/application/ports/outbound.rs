use crate::application::error::Result;
use crate::domain::entities::Gesture;
use async_trait::async_trait;
use serde_json::Value;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait FlutterVmPort: Send + Sync {
    /// Conecta con el Dart VM Service vía WebSocket
    async fn connect(&self, vm_service_uri: &str) -> Result<()>;

    /// Desconecta de la sesión activa
    async fn disconnect(&self) -> Result<()>;

    /// Verifica si la conexión está viva
    async fn is_connected(&self) -> bool;

    /// Obtiene el árbol crudo de diagnósticos de Flutter (ext.flutter.inspector)
    async fn get_diagnostics_tree(&self, subtree_depth: u32) -> Result<Value>;

    /// Ejecuta un comando en la extensión Flutter Driver (ext.flutter.driver)
    async fn execute_driver_command(&self, command: &str, params: Value) -> Result<Value>;

    /// Ejecuta un gesto o interacción en el runtime
    async fn dispatch_gesture(&self, gesture: &Gesture) -> Result<()>;

    /// Dispara Hot Reload en el Isolate principal
    async fn trigger_hot_reload(&self) -> Result<()>;

    /// Dispara Hot Restart en el Isolate principal
    async fn trigger_hot_restart(&self) -> Result<()>;

    /// Captura un screenshot rasterizado en Base64 o bytes
    async fn capture_screenshot(&self) -> Result<Vec<u8>>;
}
