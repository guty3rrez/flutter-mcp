use crate::application::error::Result;
use crate::domain::entities::{Finder, FlutterError, Gesture, LogEntry};
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

    /// Obtiene el texto extraído de un widget
    async fn get_text(&self, finder: &Finder) -> Result<String>;

    /// Espera a que un widget aparezca en el árbol. `timeout_ms` viaja tal cual (en
    /// milisegundos, sin reescalar) como el campo `timeout` del protocolo Flutter Driver.
    async fn wait_for(&self, finder: &Finder, timeout_ms: u64) -> Result<()>;

    /// Espera a que un widget desaparezca del árbol. `timeout_ms` viaja tal cual (en
    /// milisegundos, sin reescalar) como el campo `timeout` del protocolo Flutter Driver.
    async fn wait_for_absent(&self, finder: &Finder, timeout_ms: u64) -> Result<()>;

    /// Dispara Hot Reload en el Isolate principal
    async fn trigger_hot_reload(&self) -> Result<()>;

    /// Dispara Hot Restart en el Isolate principal
    async fn trigger_hot_restart(&self) -> Result<()>;

    /// Captura un screenshot rasterizado en Base64 o bytes
    async fn capture_screenshot(&self) -> Result<Vec<u8>>;

    /// Devuelve el buffer completo de logs acumulados pasivamente desde la conexión
    /// (streams `Stdout`/`Stderr`/`Logging`). El filtrado/límite se aplica en la capa de aplicación.
    async fn get_logs(&self) -> Result<Vec<LogEntry>>;

    /// Devuelve el buffer completo de excepciones no manejadas detectadas desde la conexión
    /// (heurística pasiva sobre stdout/stderr, más las capturadas en modo preciso si está activo).
    /// Ver el doc de `ErrorSource` para la limitación validada: ninguno de los dos mecanismos
    /// detecta excepciones Dart/async genéricas no capturadas, solo errores de framework impresos.
    async fn get_errors(&self) -> Result<Vec<FlutterError>>;

    /// Activa/desactiva la captura precisa de excepciones vía `setExceptionPauseMode`. En teoría
    /// pausa el isolate brevemente en cada excepción no manejada para resolver su mensaje/stack
    /// exacto y lo reanuda de inmediato — pero validado contra una app Flutter real, no disparó
    /// `PauseException` para una excepción async genérica no capturada (ver doc de `ErrorSource`).
    async fn enable_precise_error_mode(&self, enabled: bool) -> Result<()>;

    /// Devuelve los eventos crudos del stream `Timeline` acumulados (formato Chrome Trace Event),
    /// para que la capa de aplicación los transforme en `FrameTiming`/`PerformanceReport`.
    async fn get_raw_timeline_events(&self) -> Result<Vec<Value>>;
}

/// Puerto secundario para leer/escribir archivos del proyecto Flutter en disco. Usado
/// únicamente por `flutter_start_control` para inyectar Flutter Driver en el entrypoint y, de
/// ser necesario, declarar la dependencia en `pubspec.yaml` — separado de `FlutterVmPort` porque
/// no tiene nada que ver con el Dart VM Service.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProjectFilesPort: Send + Sync {
    /// Lee el contenido completo de un archivo como texto UTF-8.
    async fn read_to_string(&self, path: &str) -> Result<String>;

    /// Sobreescribe un archivo con el contenido dado (lo crea si no existe).
    async fn write_string(&self, path: &str, content: &str) -> Result<()>;
}
