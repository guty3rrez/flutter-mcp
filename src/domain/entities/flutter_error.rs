use serde::{Deserialize, Serialize};

/// Mecanismo por el que se detectó una excepción no manejada.
///
/// **Limitación validada contra una app Flutter real (Linux desktop):** ninguno de los dos
/// mecanismos detecta excepciones Dart/async genéricas no capturadas. `StdoutHeuristic` solo ve
/// texto que la app efectivamente imprime vía `dart:io`/`print()`/`debugPrint()` (por eso sí
/// atrapa red screens de framework reportados por `FlutterError.onError`, default o custom-que-
/// imprime) — el mensaje nativo `[ERROR:flutter/runtime/dart_vm_initializer.cc] Unhandled
/// Exception: ...` que el engine escribe directo a stderr nativo nunca pasa por ahí, así que
/// jamás se detecta así. `ExceptionPause` tampoco lo capturó en esa misma prueba: se activó
/// `setExceptionPauseMode` correctamente pero nunca llegó un evento `PauseException` para una
/// excepción lanzada dentro de un `Future` — el engine parece "manejarla" internamente (vía su
/// propio listener de errores del isolate) antes de que el debugger la considere no-manejada.
/// Ver `docs/` o el README para la nota de producto sobre este límite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ErrorSource {
    /// Detectado parseando el texto de stdout/stderr (red screen / stack trace). No invasivo.
    /// Solo ve lo que la app imprime — no excepciones genéricas no capturadas (ver arriba).
    StdoutHeuristic,
    /// Detectado pausando el isolate en la excepción (`setExceptionPauseMode`). En teoría más
    /// preciso, pero validado como poco confiable para excepciones async genéricas (ver arriba)
    /// y, cuando sí dispara, introduce una pausa breve de la UI mientras se resuelve el objeto.
    ExceptionPause,
}

/// Una excepción no manejada detectada en la app conectada
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlutterError {
    pub timestamp_ms: i64,
    pub message: String,
    pub stack_trace: Option<String>,
    pub source: ErrorSource,
    /// Cuántas veces se repitió este mismo error consecutivamente (se colapsan duplicados
    /// para no inundar el contexto del agente con el mismo error de un loop de build)
    pub occurrences: u32,
}
