use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::application::error::{ApplicationError, Result};
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{ErrorSource, Finder, FlutterError, Gesture, LogEntry};
use crate::domain::services::{ErrorDetector, LogParser};

type WsSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsReceiver = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;
type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<std::result::Result<Value, String>>>>>;

/// Capacidad máxima de líneas de log retenidas en memoria (ring buffer, se descarta lo más viejo)
const LOG_BUFFER_CAPACITY: usize = 2000;
/// Capacidad máxima de errores distintos retenidos (los duplicados consecutivos se colapsan)
const ERROR_BUFFER_CAPACITY: usize = 200;
/// Streams del VM Service a los que nos suscribimos al conectar. Best-effort: si alguno falla
/// (versión de engine sin ese stream, etc.) se registra un warning y se continúa sin él.
/// `Timeline` no está acá a propósito: validado contra una app Flutter real, los eventos de
/// timeline llegaban al buffer interno del VM pero el push por `streamNotify` no se entregaba
/// de forma confiable en la ventana de una sesión de depuración corta — se lee por pull
/// (`getVMTimeline`) en `get_raw_timeline_events` en su lugar, que sí devolvió el buffer completo.
const SUBSCRIBED_STREAMS: &[&str] = &["Stdout", "Stderr", "Logging", "Debug"];

/// Estado compartido entre el adaptador y el task lector en background. Se agrupa en un solo
/// struct (barato de clonar, son todo `Arc`) para que las funciones de bajo nivel que necesitan
/// emitir RPCs o volcar eventos de stream no terminen con media docena de parámetros sueltos.
#[derive(Clone)]
struct VmConnectionState {
    sender: Arc<Mutex<Option<WsSender>>>,
    pending: PendingMap,
    request_counter: Arc<AtomicU64>,
    main_isolate_id: Arc<Mutex<Option<String>>>,
    logs: Arc<Mutex<VecDeque<LogEntry>>>,
    errors: Arc<Mutex<VecDeque<FlutterError>>>,
    error_detector: Arc<Mutex<ErrorDetector>>,
    precise_errors_enabled: Arc<AtomicBool>,
}

impl VmConnectionState {
    fn new() -> Self {
        Self {
            sender: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            request_counter: Arc::new(AtomicU64::new(1)),
            main_isolate_id: Arc::new(Mutex::new(None)),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            errors: Arc::new(Mutex::new(VecDeque::new())),
            error_detector: Arc::new(Mutex::new(ErrorDetector::new())),
            precise_errors_enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cierra un bloque de error en curso (ver `ErrorDetector::flush`) y lo agrega al buffer
    /// antes de leerlo, para no perder una excepción cuyo stack trace fue lo último impreso.
    async fn flush_pending_error(&self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if let Some(flushed) = self.error_detector.lock().await.flush(now_ms) {
            push_error(&self.errors, flushed).await;
        }
    }

    async fn main_isolate_id_or_default(&self) -> String {
        self.main_isolate_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| "isolates/main".into())
    }

    /// RPC de bajo nivel. Sin timeout explícito: algunas RPC (waitFor/waitForAbsent/
    /// scrollUntilVisible) delegan su propio timeout a Flutter Driver y pueden tardar
    /// legítimamente lo que el agente pida. Si la conexión se cae, el task lector drena
    /// `pending` y esto se desbloquea con ConnectionError en vez de colgarse para siempre.
    async fn send_rpc_raw(&self, method: &str, params: Value) -> Result<Value> {
        let id = self
            .request_counter
            .fetch_add(1, Ordering::SeqCst)
            .to_string();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        let send_result = {
            let mut sender_guard = self.sender.lock().await;
            let s = sender_guard
                .as_mut()
                .ok_or(ApplicationError::NotConnected)?;
            s.send(Message::Text(request.to_string().into())).await
        };

        if let Err(e) = send_result {
            self.pending.lock().await.remove(&id);
            return Err(ApplicationError::ConnectionError(format!(
                "Error enviando RPC: {e}"
            )));
        }

        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(rpc_error)) => Err(ApplicationError::DriverError(rpc_error)),
            Err(_) => Err(ApplicationError::ConnectionError(
                "Conexión cerrada antes de recibir respuesta RPC".into(),
            )),
        }
    }
}

/// Adaptador secundario que implementa FlutterVmPort comunicándose vía WebSocket JSON-RPC con la Dart VM
pub struct WebSocketVmServiceAdapter {
    connected: Arc<AtomicBool>,
    state: VmConnectionState,
    reader_task: Mutex<Option<JoinHandle<()>>>,
}

impl WebSocketVmServiceAdapter {
    pub fn new() -> Self {
        Self {
            connected: Arc::new(AtomicBool::new(false)),
            state: VmConnectionState::new(),
            reader_task: Mutex::new(None),
        }
    }

    async fn send_rpc(&self, method: &str, params: Value) -> Result<Value> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        self.state.send_rpc_raw(method, params).await
    }

    /// Suscribe a un stream del VM Service. Falla en silencio (solo warning) porque no todos
    /// los streams están garantizados en toda versión de engine, y no queremos romper la
    /// conexión completa por uno que no esté disponible.
    async fn try_stream_listen(&self, stream_id: &str) {
        if let Err(e) = self
            .send_rpc("streamListen", json!({ "streamId": stream_id }))
            .await
        {
            tracing::warn!("No se pudo suscribir al stream '{stream_id}': {e}");
        }
    }
}

impl Default for WebSocketVmServiceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

async fn push_bounded<T>(buffer: &Arc<Mutex<VecDeque<T>>>, item: T, capacity: usize) {
    let mut guard = buffer.lock().await;
    if guard.len() >= capacity {
        guard.pop_front();
    }
    guard.push_back(item);
}

/// Empuja un error detectado, colapsando repeticiones consecutivas idénticas (típico de un
/// mismo error repitiéndose en un loop de build) en un solo registro con `occurrences` creciente.
async fn push_error(errors: &Arc<Mutex<VecDeque<FlutterError>>>, new_error: FlutterError) {
    let mut guard = errors.lock().await;
    if let Some(last) = guard.back_mut()
        && last.message == new_error.message
        && last.source == new_error.source
    {
        last.occurrences += 1;
        last.timestamp_ms = new_error.timestamp_ms;
        return;
    }
    if guard.len() >= ERROR_BUFFER_CAPACITY {
        guard.pop_front();
    }
    guard.push_back(new_error);
}

async fn handle_stream_event(parsed: &Value, state: &VmConnectionState) {
    let Some(params) = parsed.get("params") else {
        return;
    };
    let Some(stream_id) = params.get("streamId").and_then(|s| s.as_str()) else {
        return;
    };
    let Some(event) = params.get("event") else {
        return;
    };

    match stream_id {
        "Stdout" | "Stderr" => {
            let is_stderr = stream_id == "Stderr";
            if let Some(entry) = LogParser::parse_stdout_event(is_stderr, event) {
                let timestamp_ms = entry.timestamp_ms;
                let message = entry.message.clone();
                push_bounded(&state.logs, entry, LOG_BUFFER_CAPACITY).await;

                let mut detector = state.error_detector.lock().await;
                for line in message.lines() {
                    if let Some(detected) = detector.process_line(line, timestamp_ms) {
                        push_error(&state.errors, detected).await;
                    }
                }
            }
        }
        "Logging" => {
            if let Some(entry) = LogParser::parse_logging_event(event) {
                push_bounded(&state.logs, entry, LOG_BUFFER_CAPACITY).await;
            }
        }
        "Debug" => {
            if !state.precise_errors_enabled.load(Ordering::SeqCst) {
                return;
            }
            if let Some("PauseException") = event.get("kind").and_then(|k| k.as_str()) {
                let isolate_id = state.main_isolate_id_or_default().await;
                let detected = resolve_pause_exception(event, &isolate_id, state).await;
                push_error(&state.errors, detected).await;
            }
        }
        _ => {}
    }
}

/// Resuelve el mensaje/stack de una excepción que pausó el isolate (`PauseException`) y lo
/// reanuda de inmediato. Prioriza minimizar el tiempo pausado por sobre la exhaustividad de la
/// resolución: si `invoke`/`getStack` fallan, igual se registra el error con lo que se tenga.
async fn resolve_pause_exception(
    event: &Value,
    isolate_id: &str,
    state: &VmConnectionState,
) -> FlutterError {
    let timestamp_ms = event.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0);
    let exception_id = event
        .get("exception")
        .and_then(|e| e.get("id"))
        .and_then(|id| id.as_str())
        .map(ToString::to_string);

    let message = match &exception_id {
        Some(id) => resolve_exception_message(state, isolate_id, id)
            .await
            .unwrap_or_else(|| "Excepción no capturada (no se pudo resolver el mensaje)".into()),
        None => "Excepción no capturada (sin referencia de objeto)".into(),
    };

    let stack_trace = resolve_stack_trace(state, isolate_id).await;

    let _ = state
        .send_rpc_raw("resume", json!({ "isolateId": isolate_id }))
        .await;

    FlutterError {
        timestamp_ms,
        message,
        stack_trace,
        source: ErrorSource::ExceptionPause,
        occurrences: 1,
    }
}

async fn resolve_exception_message(
    state: &VmConnectionState,
    isolate_id: &str,
    exception_id: &str,
) -> Option<String> {
    let result = state
        .send_rpc_raw(
            "invoke",
            json!({
                "isolateId": isolate_id,
                "targetId": exception_id,
                "selector": "toString",
                "argumentIds": []
            }),
        )
        .await
        .ok()?;

    result
        .get("valueAsString")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

async fn resolve_stack_trace(state: &VmConnectionState, isolate_id: &str) -> Option<String> {
    let result = state
        .send_rpc_raw("getStack", json!({ "isolateId": isolate_id }))
        .await
        .ok()?;

    let frames = result.get("frames")?.as_array()?;
    let lines: Vec<String> = frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let loc = f
                .get("code")
                .and_then(|c| c.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("<desconocido>");
            format!("#{i}      {loc}")
        })
        .collect();

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Task lector único con posesión exclusiva del `WsReceiver`. Demultiplexa respuestas de RPC
/// (mensajes con "id", resueltos vía la tabla `pending`) de eventos no solicitados
/// (`streamNotify`, sin "id", despachados a los buffers de logs/errores/timeline).
async fn run_reader_loop(
    mut receiver: WsReceiver,
    connected: Arc<AtomicBool>,
    state: VmConnectionState,
) {
    while let Some(msg_result) = receiver.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(_) => break,
        };
        let Message::Text(text) = msg else { continue };
        let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
            continue;
        };

        if let Some(id) = parsed.get("id").and_then(|i| i.as_str()) {
            if let Some(tx) = state.pending.lock().await.remove(id) {
                let outcome = if let Some(error) = parsed.get("error") {
                    Err(error.to_string())
                } else {
                    Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
                };
                let _ = tx.send(outcome);
            }
            continue;
        }

        if parsed.get("method").and_then(|m| m.as_str()) == Some("streamNotify") {
            handle_stream_event(&parsed, &state).await;
        }
    }

    // La conexión se cayó: desbloquear cualquier RPC en vuelo con un error en vez de dejarla
    // colgada para siempre (nadie más va a completar esos oneshot).
    state.pending.lock().await.clear();
    connected.store(false, Ordering::SeqCst);
}

#[async_trait]
impl FlutterVmPort for WebSocketVmServiceAdapter {
    async fn connect(&self, vm_service_uri: &str) -> Result<()> {
        let (ws_stream, _) = connect_async(vm_service_uri).await.map_err(|e| {
            ApplicationError::ConnectionError(format!(
                "No se pudo conectar a {vm_service_uri}: {e}"
            ))
        })?;

        let (s, r) = ws_stream.split();
        *self.state.sender.lock().await = Some(s);
        self.connected.store(true, Ordering::SeqCst);

        let reader_handle = tokio::spawn(run_reader_loop(
            r,
            self.connected.clone(),
            self.state.clone(),
        ));
        *self.reader_task.lock().await = Some(reader_handle);

        // Autodescubrir Isolate
        let vm_info = self.send_rpc("getVM", json!({})).await?;
        let first_isolate = vm_info
            .get("isolates")
            .and_then(|i| i.as_array())
            .and_then(|isolates| isolates.first())
            .and_then(|iso| iso.get("id"))
            .and_then(|id| id.as_str());
        if let Some(id) = first_isolate {
            *self.state.main_isolate_id.lock().await = Some(id.to_string());
        }

        for stream_id in SUBSCRIBED_STREAMS {
            self.try_stream_listen(stream_id).await;
        }
        if let Err(e) = self
            .send_rpc(
                "setVMTimelineFlags",
                json!({ "recordedStreams": ["Dart", "Embedder", "GC", "Compiler"] }),
            )
            .await
        {
            tracing::warn!("No se pudo configurar el timeline de VM: {e}");
        }

        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        self.state
            .precise_errors_enabled
            .store(false, Ordering::SeqCst);

        if let Some(handle) = self.reader_task.lock().await.take() {
            handle.abort();
        }

        let mut sender = self.state.sender.lock().await;
        if let Some(mut s) = sender.take() {
            let _ = s.close().await;
        }
        drop(sender);

        self.state.pending.lock().await.clear();
        self.state.logs.lock().await.clear();
        self.state.errors.lock().await.clear();
        *self.state.error_detector.lock().await = ErrorDetector::new();

        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn get_diagnostics_tree(&self, subtree_depth: u32) -> Result<Value> {
        let isolate_id = self.state.main_isolate_id_or_default().await;

        self.send_rpc(
            "ext.flutter.inspector.getRootWidgetSummaryTree",
            json!({
                "isolateId": isolate_id,
                "objectGroup": "flutter-native-mcp",
                "subtreeDepth": subtree_depth
            }),
        )
        .await
    }

    async fn execute_driver_command(&self, command: &str, mut params: Value) -> Result<Value> {
        let isolate_id = self.state.main_isolate_id_or_default().await;

        if let Some(obj) = params.as_object_mut() {
            obj.insert("command".into(), json!(command));
            obj.insert("isolateId".into(), json!(isolate_id));
        }

        self.send_rpc("ext.flutter.driver", params).await
    }

    async fn dispatch_gesture(&self, gesture: &Gesture) -> Result<()> {
        match gesture {
            Gesture::Tap { finder } => {
                let mut map = finder.to_driver_params();
                map.insert("timeout".into(), json!("5000"));
                self.execute_driver_command("tap", Value::Object(map))
                    .await?;
                Ok(())
            }
            Gesture::EnterText { text, .. } => {
                let params = json!({
                    "text": text,
                    "timeout": "5000"
                });
                self.execute_driver_command("enter_text", params).await?;
                Ok(())
            }
            Gesture::ClearText { .. } => {
                let params = json!({
                    "text": "",
                    "timeout": "5000"
                });
                self.execute_driver_command("enter_text", params).await?;
                Ok(())
            }
            Gesture::Scroll {
                finder,
                dx,
                dy,
                duration_ms,
                frequency,
            } => {
                let mut map = finder.to_driver_params();
                map.insert("dx".into(), json!(dx.to_string()));
                map.insert("dy".into(), json!(dy.to_string()));
                map.insert("duration".into(), json!((duration_ms * 1000).to_string()));
                map.insert("frequency".into(), json!(frequency.to_string()));
                map.insert("timeout".into(), json!("5000"));
                self.execute_driver_command("scroll", Value::Object(map))
                    .await?;
                Ok(())
            }
            Gesture::ScrollIntoView { finder, alignment } => {
                let mut map = finder.to_driver_params();
                map.insert("alignment".into(), json!(alignment.to_string()));
                map.insert("timeout".into(), json!("5000"));
                self.execute_driver_command("scrollIntoView", Value::Object(map))
                    .await?;
                Ok(())
            }
            Gesture::ScrollUntilVisible {
                scrollable,
                target,
                delta,
                max_scrolls,
            } => {
                let scroll_finder = scrollable
                    .clone()
                    .unwrap_or_else(|| Finder::by_type("Scrollable"));
                for _ in 0..*max_scrolls {
                    let mut check_map = target.to_driver_params();
                    check_map.insert("timeout".into(), json!("500000")); // 500ms
                    if self
                        .execute_driver_command("waitFor", Value::Object(check_map))
                        .await
                        .is_ok()
                    {
                        return Ok(());
                    }
                    let mut scroll_map = scroll_finder.to_driver_params();
                    scroll_map.insert("dx".into(), json!("0"));
                    scroll_map.insert("dy".into(), json!(delta.to_string()));
                    scroll_map.insert("duration".into(), json!("300000"));
                    scroll_map.insert("frequency".into(), json!("60"));
                    scroll_map.insert("timeout".into(), json!("5000"));
                    let _ = self
                        .execute_driver_command("scroll", Value::Object(scroll_map))
                        .await;
                }
                Ok(())
            }
        }
    }

    async fn get_text(&self, finder: &Finder) -> Result<String> {
        let mut map = finder.to_driver_params();
        map.insert("timeout".into(), json!("5000"));
        let result = self
            .execute_driver_command("get_text", Value::Object(map))
            .await?;

        let text_opt = result
            .get("response")
            .and_then(|r| r.get("text"))
            .or_else(|| result.get("text"))
            .and_then(|t| t.as_str());

        if let Some(text) = text_opt {
            Ok(text.to_string())
        } else {
            Err(ApplicationError::DriverError(format!(
                "No se pudo extraer texto del widget. Respuesta: {result:?}"
            )))
        }
    }

    async fn wait_for(&self, finder: &Finder, timeout_ms: u64) -> Result<()> {
        let mut map = finder.to_driver_params();
        map.insert("timeout".into(), json!((timeout_ms * 1000).to_string()));
        self.execute_driver_command("waitFor", Value::Object(map))
            .await?;
        Ok(())
    }

    async fn wait_for_absent(&self, finder: &Finder, timeout_ms: u64) -> Result<()> {
        let mut map = finder.to_driver_params();
        map.insert("timeout".into(), json!((timeout_ms * 1000).to_string()));
        self.execute_driver_command("waitForAbsent", Value::Object(map))
            .await?;
        Ok(())
    }

    async fn trigger_hot_reload(&self) -> Result<()> {
        let isolate_id = self.state.main_isolate_id_or_default().await;
        self.send_rpc("reloadSources", json!({ "isolateId": isolate_id }))
            .await?;
        Ok(())
    }

    async fn trigger_hot_restart(&self) -> Result<()> {
        let isolate_id = self.state.main_isolate_id_or_default().await;

        if self
            .send_rpc("ext.flutter.reassemble", json!({ "isolateId": isolate_id }))
            .await
            .is_ok()
        {
            return Ok(());
        }

        self.send_rpc("hotRestart", json!({})).await?;
        Ok(())
    }

    async fn capture_screenshot(&self) -> Result<Vec<u8>> {
        let result = self
            .execute_driver_command("screenshot", json!({ "timeout": "5000" }))
            .await?;

        let b64_opt = result
            .get("response")
            .and_then(|r| r.get("data"))
            .or_else(|| {
                result.get("data").and_then(|d| {
                    if d.is_object() {
                        d.get("data")
                    } else {
                        Some(d)
                    }
                })
            })
            .or_else(|| result.get("screenshot"))
            .and_then(|s| s.as_str());

        if let Some(screenshot_b64) = b64_opt {
            use base64::prelude::*;
            BASE64_STANDARD.decode(screenshot_b64.trim()).map_err(|e| {
                ApplicationError::DriverError(format!("Error decodificando screenshot Base64: {e}"))
            })
        } else {
            Err(ApplicationError::DriverError(format!(
                "No se recibió captura en respuesta. Respuesta: {result:?}"
            )))
        }
    }

    async fn get_logs(&self) -> Result<Vec<LogEntry>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(self.state.logs.lock().await.iter().cloned().collect())
    }

    async fn get_errors(&self) -> Result<Vec<FlutterError>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        self.state.flush_pending_error().await;
        Ok(self.state.errors.lock().await.iter().cloned().collect())
    }

    async fn enable_precise_error_mode(&self, enabled: bool) -> Result<()> {
        let isolate_id = self.state.main_isolate_id_or_default().await;
        let mode = if enabled { "Unhandled" } else { "None" };
        self.send_rpc(
            "setExceptionPauseMode",
            json!({ "isolateId": isolate_id, "mode": mode }),
        )
        .await?;
        self.state
            .precise_errors_enabled
            .store(enabled, Ordering::SeqCst);
        Ok(())
    }

    async fn get_raw_timeline_events(&self) -> Result<Vec<Value>> {
        // Pull en vez de push: validado contra una app Flutter real, el stream `Timeline` vía
        // `streamNotify` no entregaba nada en una ventana de varios segundos con actividad real
        // de UI, mientras que `getVMTimeline` sí devolvió miles de eventos ya bufferizados por
        // el propio VM Service (recorder tipo "Ring", acotado del lado del VM).
        let result = self.send_rpc("getVMTimeline", json!({})).await?;
        Ok(result
            .get("traceEvents")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default())
    }
}
