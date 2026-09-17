use crate::application::error::Result;
use crate::domain::entities::{
    Finder, FlutterError, LogEntry, LogFilter, PerformanceReport, WidgetNode,
};
use async_trait::async_trait;

#[async_trait]
pub trait FlutterAppService: Send + Sync {
    async fn connect(&self, uri: &str) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn get_pruned_snapshot(&self) -> Result<WidgetNode>;
    async fn tap(&self, finder: Finder) -> Result<()>;
    async fn enter_text(&self, finder: Finder, text: String) -> Result<()>;
    async fn get_text(&self, finder: Finder) -> Result<String>;
    async fn scroll(
        &self,
        finder: Finder,
        dx: f64,
        dy: f64,
        duration_ms: u64,
        frequency: u32,
    ) -> Result<()>;
    async fn scroll_into_view(&self, finder: Finder, alignment: f64) -> Result<()>;
    async fn wait_for(&self, finder: Finder, timeout_ms: u64) -> Result<()>;
    async fn wait_for_absent(&self, finder: Finder, timeout_ms: u64) -> Result<()>;
    async fn hot_reload(&self) -> Result<()>;
    async fn hot_restart(&self) -> Result<()>;
    async fn take_screenshot(&self) -> Result<Vec<u8>>;

    /// Lee los logs acumulados desde la conexión, aplicando filtro por texto/origen y límite.
    async fn get_logs(&self, filter: LogFilter) -> Result<Vec<LogEntry>>;

    /// Lee las excepciones de framework detectadas (red screens impresos). Si `precise` es
    /// `true` y el modo preciso no estaba activo, lo activa para la sesión. Detecta solo lo que
    /// la app imprime por stdout/stderr — no excepciones Dart/async genéricas no capturadas
    /// (ver `domain::entities::ErrorSource` para la limitación validada contra una app real).
    async fn get_errors(&self, precise: bool) -> Result<Vec<FlutterError>>;

    /// Deriva un reporte de rendimiento (jank, promedios de build/raster) sobre los eventos de
    /// timeline acumulados, opcionalmente acotado a los últimos `window_ms` milisegundos.
    async fn get_performance(&self, window_ms: Option<u64>) -> Result<PerformanceReport>;
}
