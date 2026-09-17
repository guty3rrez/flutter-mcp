use serde::{Deserialize, Serialize};

/// Origen de una línea de log capturada desde la Dart VM
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogSource {
    Stdout,
    Stderr,
    Logging {
        level: Option<i64>,
        logger_name: Option<String>,
    },
}

/// Una línea de log capturada de forma pasiva desde que se estableció la conexión
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp_ms: i64,
    pub source: LogSource,
    pub message: String,
}

impl LogEntry {
    pub fn source_label(&self) -> &'static str {
        match &self.source {
            LogSource::Stdout => "stdout",
            LogSource::Stderr => "stderr",
            LogSource::Logging { .. } => "logging",
        }
    }
}

/// Filtro opcional aplicado sobre el buffer de logs acumulado
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub contains: Option<String>,
    pub source: Option<String>,
    pub limit: usize,
}
