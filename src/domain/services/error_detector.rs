use crate::domain::entities::{ErrorSource, FlutterError};

const EXCEPTION_BANNER_MARKER: &str = "EXCEPTION CAUGHT BY";
const SEPARATOR_CHAR: char = '═';
/// Substring case-insensitive, no línea exacta: cubre tanto el `dart run` puro ("Unhandled
/// exception:" en su propia línea) como el real de Flutter engine, que va en una sola línea
/// con prefijo: "[ERROR:flutter/runtime/dart_vm_initializer.cc(40)] Unhandled Exception: <msg>"
/// (validado contra una app Flutter real corriendo en Linux).
const UNHANDLED_MARKER: &str = "unhandled exception";
/// Salvaguarda ante un formato de error inesperado que nunca cierra el bloque: se corta igual
/// para no acumular líneas de stdout no relacionadas indefinidamente dentro de un mismo error.
const MAX_BUFFERED_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
enum CaptureKind {
    /// Bloque delimitado por la banda `══╡ EXCEPTION CAUGHT BY ... ╞══` ... `═════`
    FlutterBanner,
    /// Excepción no capturada sin red screen, en cualquiera de sus dos formatos reales:
    /// "Unhandled exception:" en línea propia (dart run) o combinada con el mensaje en una
    /// sola línea con prefijo `[ERROR:...]` (Flutter engine) — seguida de un stack `#0 ...`
    PlainUnhandled,
}

/// Detecta excepciones de framework (red screens de build/layout/paint) parseando línea a línea
/// el texto de stdout/stderr que la app ya imprime (vía `debugPrint`/`print`, sea el
/// `FlutterError.onError` por defecto o uno personalizado que también imprima). Es un
/// heurístico sobre el formato de texto de Flutter/Dart, no un contrato de protocolo.
///
/// **No detecta excepciones Dart/async genéricas no capturadas** — validado contra una app
/// Flutter real, el mensaje nativo que el engine escribe a stderr para esos casos
/// (`[ERROR:flutter/runtime/dart_vm_initializer.cc] Unhandled Exception: ...`) nunca pasa por el
/// sink `dart:io` que el stream `Stdout`/`Stderr` del VM Service observa. Ver `ErrorSource` para
/// el detalle completo, incluyendo por qué `ExceptionPause` tampoco lo resuelve de forma confiable.
#[derive(Debug, Default)]
pub struct ErrorDetector {
    capturing: Option<CaptureKind>,
    buffer: Vec<String>,
    seen_stack_frame: bool,
}

impl ErrorDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Alimenta una línea de stdout/stderr. Devuelve un `FlutterError` completo cuando la
    /// línea cierra un bloque de error que se venía acumulando.
    pub fn process_line(&mut self, line: &str, timestamp_ms: i64) -> Option<FlutterError> {
        let trimmed = line.trim_end();

        match self.capturing {
            None => {
                if trimmed.contains(EXCEPTION_BANNER_MARKER) {
                    self.start(CaptureKind::FlutterBanner, trimmed);
                } else if trimmed.to_lowercase().contains(UNHANDLED_MARKER) {
                    self.start(CaptureKind::PlainUnhandled, trimmed);
                }
                None
            }
            Some(CaptureKind::FlutterBanner) => {
                let is_pure_separator =
                    !trimmed.is_empty() && trimmed.chars().all(|c| c == SEPARATOR_CHAR);
                if is_pure_separator {
                    return Some(self.finish(timestamp_ms));
                }
                self.push_or_force_close(trimmed, timestamp_ms)
            }
            Some(CaptureKind::PlainUnhandled) => {
                let is_stack_frame = trimmed.trim_start().starts_with('#');
                if is_stack_frame {
                    self.seen_stack_frame = true;
                    return self.push_or_force_close(trimmed, timestamp_ms);
                }
                if trimmed.trim().is_empty() || self.seen_stack_frame {
                    return Some(self.finish(timestamp_ms));
                }
                self.push_or_force_close(trimmed, timestamp_ms)
            }
        }
    }

    /// Fuerza el cierre de un bloque en curso. Necesario porque el caso `PlainUnhandled` cierra
    /// recién con la línea siguiente a la última línea de stack (no hay un separador explícito
    /// como en el red screen de Flutter) — si esa excepción fue lo último que la app imprimió,
    /// nunca llegaría otra línea que la cierre. El llamador debe invocar esto antes de leer el
    /// resultado acumulado si quiere ver también la excepción en curso.
    pub fn flush(&mut self, timestamp_ms: i64) -> Option<FlutterError> {
        if self.capturing.is_some() {
            Some(self.finish(timestamp_ms))
        } else {
            None
        }
    }

    fn start(&mut self, kind: CaptureKind, first_line: &str) {
        self.capturing = Some(kind);
        self.buffer.clear();
        self.buffer.push(first_line.to_string());
        self.seen_stack_frame = false;
    }

    fn push_or_force_close(&mut self, line: &str, timestamp_ms: i64) -> Option<FlutterError> {
        self.buffer.push(line.to_string());
        if self.buffer.len() > MAX_BUFFERED_LINES {
            return Some(self.finish(timestamp_ms));
        }
        None
    }

    fn finish(&mut self, timestamp_ms: i64) -> FlutterError {
        self.capturing = None;
        let full = self.buffer.join("\n");
        self.buffer.clear();
        let (message, stack_trace) = Self::split_message_and_stack(&full);
        FlutterError {
            timestamp_ms,
            message,
            stack_trace,
            source: ErrorSource::StdoutHeuristic,
            occurrences: 1,
        }
    }

    /// La primera línea "sustancial" (no puramente separadores) del bloque es el resumen;
    /// el resto queda como detalle/stack trace.
    fn split_message_and_stack(full: &str) -> (String, Option<String>) {
        let mut lines = full
            .lines()
            .filter(|l| !(!l.is_empty() && l.chars().all(|c| c == SEPARATOR_CHAR)));
        let message = lines.next().unwrap_or(full).trim().to_string();
        let rest: Vec<&str> = lines.collect();
        let stack_trace = if rest.is_empty() {
            None
        } else {
            Some(rest.join("\n"))
        };
        (message, stack_trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(detector: &mut ErrorDetector, lines: &[&str]) -> Option<FlutterError> {
        let mut result = None;
        for line in lines {
            if let Some(err) = detector.process_line(line, 1_000) {
                result = Some(err);
            }
        }
        result
    }

    #[test]
    fn test_detects_flutter_red_screen_block() {
        let mut detector = ErrorDetector::new();
        let lines = [
            "══╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞══════════════════════",
            "The following _CastError was thrown building MyWidget:",
            "Null check operator used on a null value",
            "═══════════════════════════════════════════════════════════════",
        ];

        let error = feed(&mut detector, &lines).expect("Debe detectar un error");
        assert_eq!(error.source, ErrorSource::StdoutHeuristic);
        assert!(error.message.contains("EXCEPTION CAUGHT BY"));
        assert!(
            error
                .stack_trace
                .as_deref()
                .unwrap()
                .contains("Null check operator")
        );
    }

    #[test]
    fn test_detects_plain_unhandled_exception() {
        let mut detector = ErrorDetector::new();
        let lines = [
            "Unhandled exception:",
            "Exception: algo salió mal",
            "#0      main.<anonymous closure> (file.dart:10:5)",
            "#1      main (file.dart:20:3)",
            "Otra línea de stdout no relacionada",
        ];

        let error = feed(&mut detector, &lines).expect("Debe detectar un error");
        assert_eq!(error.message, "Unhandled exception:");
        let stack = error.stack_trace.expect("Debe tener stack trace");
        assert!(stack.contains("Exception: algo salió mal"));
        assert!(stack.contains("#0"));
        assert!(!stack.contains("Otra línea"));
    }

    #[test]
    fn test_detects_flutter_engine_unhandled_exception_format() {
        // Formato real observado corriendo `flutter run -d linux`: mensaje y prefijo van en la
        // misma línea (a diferencia del "Unhandled exception:" propio de `dart run`), y la letra
        // de "Exception" va en mayúscula.
        let mut detector = ErrorDetector::new();
        let lines = [
            "[ERROR:flutter/runtime/dart_vm_initializer.cc(40)] Unhandled Exception: Exception: probe de validación",
            "#0      Eval.<anonymous closure>.<anonymous closure> ()",
            "#1      new Future.delayed.<anonymous closure> (dart:async/future.dart:448:42)",
            "media_kit: NativeReferenceHolder: Allocated 12345",
        ];

        let error = feed(&mut detector, &lines).expect("Debe detectar un error");
        assert_eq!(error.source, ErrorSource::StdoutHeuristic);
        assert!(
            error
                .message
                .contains("Unhandled Exception: Exception: probe de validación")
        );
        let stack = error.stack_trace.expect("Debe tener stack trace");
        assert!(stack.contains("#0"));
        assert!(!stack.contains("media_kit"));
    }

    #[test]
    fn test_ignores_unrelated_lines() {
        let mut detector = ErrorDetector::new();
        assert!(
            detector
                .process_line("Just a normal print statement", 1)
                .is_none()
        );
        assert!(detector.process_line("Another one", 2).is_none());
    }
}
