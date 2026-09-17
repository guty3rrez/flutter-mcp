use crate::domain::entities::{LogEntry, LogSource};
use serde_json::Value;

pub struct LogParser;

impl LogParser {
    /// Decodifica un evento `WriteEvent` de los streams `Stdout`/`Stderr` del VM Service.
    /// El texto real viene en el campo `bytes`, codificado en Base64.
    pub fn parse_stdout_event(is_stderr: bool, event: &Value) -> Option<LogEntry> {
        let encoded = event.get("bytes").and_then(|b| b.as_str())?;
        use base64::prelude::*;
        let bytes = BASE64_STANDARD.decode(encoded).ok()?;
        let message = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\n')
            .to_string();
        if message.is_empty() {
            return None;
        }

        let timestamp_ms = event.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0);
        Some(LogEntry {
            timestamp_ms,
            source: if is_stderr {
                LogSource::Stderr
            } else {
                LogSource::Stdout
            },
            message,
        })
    }

    /// Extrae un `LogEntry` de un evento del stream `Logging` (llamadas a `dart:developer.log()`).
    /// Solo funciona para apps que ya usan `dart:developer.log()` o un logger que lo envuelva;
    /// `print()` normal no pasa por aquí, pasa por `parse_stdout_event`.
    pub fn parse_logging_event(event: &Value) -> Option<LogEntry> {
        let record = event.get("logRecord")?;
        let message = Self::instance_ref_string(record.get("message")?)?;
        let level = record.get("level").and_then(|l| l.as_i64());
        let logger_name = record
            .get("loggerName")
            .and_then(Self::instance_ref_string)
            .filter(|s| !s.is_empty());
        let timestamp_ms = record.get("time").and_then(|t| t.as_i64()).unwrap_or(0);

        Some(LogEntry {
            timestamp_ms,
            source: LogSource::Logging { level, logger_name },
            message,
        })
    }

    /// Los `InstanceRef` de String cortas traen el valor inline en `valueAsString`; si la
    /// cadena es demasiado larga el VM Service la trunca y no la trae inline. Se opta por
    /// devolver `None` en ese caso en vez de hacer un round-trip adicional con `getObject`.
    fn instance_ref_string(value: &Value) -> Option<String> {
        value
            .get("valueAsString")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_stdout_event_decodes_base64() {
        use base64::prelude::*;
        let event = json!({
            "bytes": BASE64_STANDARD.encode("Hola mundo\n"),
            "timestamp": 1_700_000_000_000i64
        });

        let entry = LogParser::parse_stdout_event(false, &event).expect("Debe parsear");
        assert_eq!(entry.message, "Hola mundo");
        assert_eq!(entry.source, LogSource::Stdout);
        assert_eq!(entry.timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn test_parse_stdout_event_marks_stderr() {
        use base64::prelude::*;
        let event = json!({ "bytes": BASE64_STANDARD.encode("boom"), "timestamp": 1 });
        let entry = LogParser::parse_stdout_event(true, &event).expect("Debe parsear");
        assert_eq!(entry.source, LogSource::Stderr);
    }

    #[test]
    fn test_parse_stdout_event_ignores_empty_lines() {
        use base64::prelude::*;
        let event = json!({ "bytes": BASE64_STANDARD.encode("\n"), "timestamp": 1 });
        assert!(LogParser::parse_stdout_event(false, &event).is_none());
    }

    #[test]
    fn test_parse_logging_event_extracts_structured_fields() {
        let event = json!({
            "logRecord": {
                "message": { "valueAsString": "usuario autenticado" },
                "level": 800,
                "loggerName": { "valueAsString": "auth" },
                "time": 1_700_000_000_500i64
            }
        });

        let entry = LogParser::parse_logging_event(&event).expect("Debe parsear");
        assert_eq!(entry.message, "usuario autenticado");
        match entry.source {
            LogSource::Logging { level, logger_name } => {
                assert_eq!(level, Some(800));
                assert_eq!(logger_name.as_deref(), Some("auth"));
            }
            _ => panic!("Se esperaba LogSource::Logging"),
        }
    }

    #[test]
    fn test_parse_logging_event_without_message_returns_none() {
        let event = json!({ "logRecord": { "level": 800 } });
        assert!(LogParser::parse_logging_event(&event).is_none());
    }
}
