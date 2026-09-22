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
use crate::domain::entities::{ErrorSource, Finder, FlutterError, Gesture, LogEntry, WidgetNode};
use crate::domain::services::{
    ErrorDetector, FinderResolver, LogParser, ResolveOutcome, TreePruner,
};

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

/// Timeout normal para gestos vía Flutter Driver cuando el pre-chequeo (`precheck_finder`)
/// confirma un match, o no aplica (finders `key`/`type`/`coordinates`) -- se espera que
/// resuelva casi al instante.
const DEFAULT_GESTURE_TIMEOUT_MS: u64 = 5000;
/// Timeout acotado para cuando el pre-chequeo no encontró ningún match. Igual se intenta el
/// comando real -- cero riesgo de falso negativo si la heurística de `TreePruner`/
/// `FinderResolver` tiene algún gap -- pero sin pagar el costo completo de 5s del caso "no
/// existe", que es el que originaba el cuelgue reportado.
const FAST_FAIL_GESTURE_TIMEOUT_MS: u64 = 800;
/// Profundidad de árbol usada para el pre-chequeo -- misma que usa `get_pruned_snapshot` en la
/// capa de aplicación para `flutter_snapshot`, así el pre-chequeo ve exactamente lo mismo.
const PRECHECK_TREE_DEPTH: u32 = 50;

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
    /// `true` una vez que `set_frame_sync(false)` y `set_text_entry_emulation(true)` fueron
    /// aplicados exitosamente contra la instancia ACTUAL de `FlutterDriverExtension`. Un Hot
    /// Restart recrea esa instancia con sus defaults (frameSync=true, sin emulación) porque
    /// re-ejecuta `main()`, así que `trigger_hot_restart`/`disconnect` resetean este flag.
    driver_extension_configured: Arc<AtomicBool>,
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
            driver_extension_configured: Arc::new(AtomicBool::new(false)),
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

    /// Envía un comando `ext.flutter.driver` tal cual, sin pasar por
    /// `ensure_driver_extension_configured` (usado tanto por el trait público como por la propia
    /// configuración lazy, para evitar recursión infinita entre ambas).
    async fn execute_driver_command_raw(&self, command: &str, mut params: Value) -> Result<Value> {
        let isolate_id = self.state.main_isolate_id_or_default().await;
        if let Some(obj) = params.as_object_mut() {
            obj.insert("command".into(), json!(command));
            obj.insert("isolateId".into(), json!(isolate_id));
        }

        let result = self.send_rpc("ext.flutter.driver", params).await?;
        if driver_result_is_error(&result) {
            return Err(ApplicationError::DriverError(driver_error_message(
                command, &result,
            )));
        }
        Ok(result)
    }

    /// Configura, una única vez por instancia activa de `FlutterDriverExtension`, frame sync
    /// desactivado (mitiga el cuelgue de 5s por animaciones perpetuas -- cursor parpadeante,
    /// spinners, backdrop filters) y emulación de teclado activada (requisito real de
    /// `enter_text` para inyectar texto en un `EditableText`). Ambas son comandos explícitos de
    /// Flutter Driver, NO un parámetro de cada comando individual -- confirmado en vivo contra
    /// una app real que el SDK ignora por completo un campo `frameSync` puesto en los params de
    /// `tap`/`scroll`/etc. Los valores van serializados como STRING ("false"/"true"), como el
    /// resto del protocolo Flutter Driver. Si la configuración falla, el flag queda en `false`
    /// para reintentar en la próxima llamada en vez de quedar falsamente marcada como lista.
    async fn ensure_driver_extension_configured(&self) -> Result<()> {
        if self
            .state
            .driver_extension_configured
            .load(Ordering::SeqCst)
        {
            return Ok(());
        }

        self.execute_driver_command_raw("set_frame_sync", json!({ "enabled": "false" }))
            .await?;
        self.execute_driver_command_raw("set_text_entry_emulation", json!({ "enabled": "true" }))
            .await?;

        self.state
            .driver_extension_configured
            .store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Pre-chequea `finder` contra el árbol de diagnóstico actual antes de despachar un comando
    /// de Flutter Driver que lo usaría (ver `FinderResolver`). Devuelve el timeout a usar
    /// (5000ms si matcheó o el finder no es de los "lentos"; 800ms si no matcheó, como
    /// salvaguarda de latencia) y, si no matcheó, los candidatos hallados para enriquecer el
    /// error si el comando real también falla. Si el pre-chequeo mismo no se puede completar
    /// (RPC caída, árbol vacío, etc.) no bloquea el flujo: cae al timeout completo de siempre,
    /// como si no se hubiese podido pre-chequear.
    async fn precheck_finder(&self, finder: &Finder) -> (u64, Vec<WidgetNode>) {
        if !FinderResolver::is_slow_finder(finder) {
            return (DEFAULT_GESTURE_TIMEOUT_MS, Vec::new());
        }

        let Ok(raw_tree) = self.get_diagnostics_tree(PRECHECK_TREE_DEPTH).await else {
            return (DEFAULT_GESTURE_TIMEOUT_MS, Vec::new());
        };
        let Some(root) = TreePruner::prune_diagnostics_tree(&raw_tree) else {
            return (DEFAULT_GESTURE_TIMEOUT_MS, Vec::new());
        };

        match FinderResolver::quick_resolve(&root, finder) {
            ResolveOutcome::Matched => (DEFAULT_GESTURE_TIMEOUT_MS, Vec::new()),
            ResolveOutcome::NotFound { candidates } => (FAST_FAIL_GESTURE_TIMEOUT_MS, candidates),
        }
    }
}

/// Si `err` es un `DriverError` y hay candidatos, les agrega una sugerencia legible al mensaje
/// -- ver `WebSocketVmServiceAdapter::precheck_finder`. No modifica otras variantes de error.
fn append_candidate_hint(err: ApplicationError, candidates: &[WidgetNode]) -> ApplicationError {
    let suggestions: Vec<String> = candidates
        .iter()
        .filter_map(|c| {
            c.text
                .clone()
                .or_else(|| c.tooltip.clone())
                .or_else(|| c.semantics_label.clone())
        })
        .collect();
    if suggestions.is_empty() {
        return err;
    }
    match err {
        ApplicationError::DriverError(msg) => ApplicationError::DriverError(format!(
            "{msg} Candidatos con texto/tooltip/semantics parecido encontrados en el árbol \
actual: {}.",
            suggestions.join(", ")
        )),
        other => other,
    }
}

impl Default for WebSocketVmServiceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Detecta si un resultado de `ext.flutter.driver` señala una falla lógica reportada por Flutter
/// Driver mismo (`Command.isError == true`) en vez de una falla de transporte JSON-RPC -- Flutter
/// Driver devuelve estas fallas (timeout interno, finder ambiguo, etc.) como una respuesta
/// JSON-RPC *exitosa*. Defensivo ante `isError` serializado como bool JSON o como string "true"
/// (todos los demás parámetros del protocolo Flutter Driver van codificados como string).
fn driver_result_is_error(result: &Value) -> bool {
    match result.get("isError") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Construye el mensaje de `ApplicationError::DriverError` a partir de un resultado con
/// `isError: true`, usando el campo `response` (el mensaje humano que arma Flutter Driver) si
/// está presente, o el JSON crudo como fallback. Si el mensaje es el timeout genérico de
/// Flutter Driver, agrega una guía: ese timeout es el comportamiento normal de Flutter Driver
/// cuando el finder nunca resuelve (no hay forma de fallar rápido sin cambiar el protocolo), y
/// el caso más común es `by: "text"`/`"tooltip"`/`"semantics"` sin match real -- por ejemplo el
/// hint text de un campo vacío no siempre matchea con `ByText`.
fn driver_error_message(command: &str, result: &Value) -> String {
    let response = result
        .get("response")
        .and_then(|r| r.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| result.to_string());
    let mut message = format!("Flutter Driver reportó un error ejecutando '{command}': {response}");
    if response.contains("TimeoutException") {
        message.push_str(
            " (el widget no se encontró dentro del timeout: usá 'flutter_snapshot' para \
confirmar que existe y qué finder le corresponde, y preferí 'by: \"key\"' sobre \
'text'/'tooltip'/'semantics' cuando el widget lo tenga -- el hint text de un campo vacío u \
otros textos no siempre matchean con esos finders)",
        );
    }
    message
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
        self.state
            .driver_extension_configured
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

    /// `getRootWidgetTree(withPreviews: true)` es una extensión más nueva del Widget
    /// Inspector -- no soportada por SDKs de Flutter muy viejos -- que devuelve en una sola
    /// llamada un `textPreview` por nodo con el contenido real de texto. La RPC legacy
    /// `getRootWidgetSummaryTree` que se usaba antes acá NO incluye ningún `properties` en la
    /// práctica (confirmado contra una app real corriendo Flutter 3.47: 0 de 128 nodos `Text`
    /// traían texto extraíble) -- esa data solo existe en `getDetailsSubtree`, que es por-nodo
    /// y haría falta uno por cada widget del árbol (N+1, inviable para un snapshot completo).
    /// `TreePruner` fue escrito contra el shape de `getDetailsSubtree` sin darse cuenta de que
    /// la RPC realmente invocada era otra -- por eso `flutter_snapshot` nunca mostró texto real
    /// contra apps reales pese a pasar sus tests (que usan fixtures con el shape equivocado).
    /// Con fallback a la RPC vieja si el SDK conectado no soporta la nueva.
    async fn get_diagnostics_tree(&self, subtree_depth: u32) -> Result<Value> {
        let isolate_id = self.state.main_isolate_id_or_default().await;

        let result = self
            .send_rpc(
                "ext.flutter.inspector.getRootWidgetTree",
                json!({
                    "isolateId": isolate_id,
                    "groupName": "flutter-native-mcp",
                    "isSummaryTree": true,
                    "withPreviews": true
                }),
            )
            .await;

        match result {
            Ok(tree) => Ok(tree),
            Err(_) => {
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
        }
    }

    /// Ejecuta un comando `ext.flutter.driver`. Antes de la primera ejecución (y de nuevo tras
    /// cada Hot Restart) aplica la configuración lazy de frame-sync/text-entry-emulation -- ver
    /// `ensure_driver_extension_configured`. Valida además `isError` en la respuesta: Flutter
    /// Driver reporta sus propias fallas (timeout interno, finder ambiguo) como una respuesta
    /// JSON-RPC *exitosa* con `isError: true`, así que devolver `Ok(result)` sin chequear eso
    /// enmascararía la falla ante quien llama (tap/get_text/wait_for/screenshot fallando en
    /// silencio con un "éxito" falso).
    async fn execute_driver_command(&self, command: &str, params: Value) -> Result<Value> {
        self.ensure_driver_extension_configured().await?;
        self.execute_driver_command_raw(command, params).await
    }

    async fn dispatch_gesture(&self, gesture: &Gesture) -> Result<()> {
        match gesture {
            Gesture::Tap { finder } => {
                let (timeout_ms, candidates) = self.precheck_finder(finder).await;
                let mut map = finder.to_driver_params();
                map.insert("timeout".into(), json!(timeout_ms.to_string()));
                self.execute_driver_command("tap", Value::Object(map))
                    .await
                    .map_err(|e| append_candidate_hint(e, &candidates))?;
                Ok(())
            }
            Gesture::EnterText { finder, text } => {
                let (timeout_ms, candidates) = self.precheck_finder(finder).await;
                let mut tap_map = finder.to_driver_params();
                tap_map.insert("timeout".into(), json!(timeout_ms.to_string()));
                self.execute_driver_command("tap", Value::Object(tap_map))
                    .await
                    .map_err(|e| append_candidate_hint(e, &candidates))?;

                let params = json!({
                    "text": text,
                    "timeout": "5000"
                });
                self.execute_driver_command("enter_text", params).await?;
                Ok(())
            }
            Gesture::ClearText { finder } => {
                let (timeout_ms, candidates) = self.precheck_finder(finder).await;
                let mut tap_map = finder.to_driver_params();
                tap_map.insert("timeout".into(), json!(timeout_ms.to_string()));
                self.execute_driver_command("tap", Value::Object(tap_map))
                    .await
                    .map_err(|e| append_candidate_hint(e, &candidates))?;

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
                let (timeout_ms, candidates) = self.precheck_finder(finder).await;
                let mut map = finder.to_driver_params();
                map.insert("dx".into(), json!(dx.to_string()));
                map.insert("dy".into(), json!(dy.to_string()));
                map.insert("duration".into(), json!((duration_ms * 1000).to_string()));
                map.insert("frequency".into(), json!(frequency.to_string()));
                map.insert("timeout".into(), json!(timeout_ms.to_string()));
                self.execute_driver_command("scroll", Value::Object(map))
                    .await
                    .map_err(|e| append_candidate_hint(e, &candidates))?;
                Ok(())
            }
            Gesture::ScrollIntoView { finder, alignment } => {
                let (timeout_ms, candidates) = self.precheck_finder(finder).await;
                let mut map = finder.to_driver_params();
                map.insert("alignment".into(), json!(alignment.to_string()));
                map.insert("timeout".into(), json!(timeout_ms.to_string()));
                self.execute_driver_command("scrollIntoView", Value::Object(map))
                    .await
                    .map_err(|e| append_candidate_hint(e, &candidates))?;
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
                    check_map.insert("timeout".into(), json!("500")); // 500ms
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
                Err(ApplicationError::WidgetNotFound(format!(
                    "scrollUntilVisible agotó {max_scrolls} scrolls (delta={delta}) sin \
encontrar el target. Puede que no exista, que el finder no matchee, o que 'scrollable' no sea \
el contenedor correcto -- confirmá con 'flutter_snapshot'."
                )))
            }
        }
    }

    async fn get_text(&self, finder: &Finder) -> Result<String> {
        let (timeout_ms, candidates) = self.precheck_finder(finder).await;
        let mut map = finder.to_driver_params();
        map.insert("timeout".into(), json!(timeout_ms.to_string()));
        let result = self
            .execute_driver_command("get_text", Value::Object(map))
            .await
            .map_err(|e| append_candidate_hint(e, &candidates))?;

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
        map.insert("timeout".into(), json!(timeout_ms.to_string()));
        self.execute_driver_command("waitFor", Value::Object(map))
            .await?;
        Ok(())
    }

    async fn wait_for_absent(&self, finder: &Finder, timeout_ms: u64) -> Result<()> {
        let mut map = finder.to_driver_params();
        map.insert("timeout".into(), json!(timeout_ms.to_string()));
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

        let reassemble_ok = self
            .send_rpc("ext.flutter.reassemble", json!({ "isolateId": isolate_id }))
            .await
            .is_ok();

        if !reassemble_ok {
            self.send_rpc("hotRestart", json!({})).await?;
        }

        // Cualquiera de las dos vías re-ejecuta main(), recreando FlutterDriverExtension con sus
        // defaults -- invalidar el flag para que el próximo execute_driver_command la reconfigure.
        self.state
            .driver_extension_configured
            .store(false, Ordering::SeqCst);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_result_is_error_detects_bool_true() {
        assert!(driver_result_is_error(&json!({ "isError": true })));
    }

    #[test]
    fn driver_result_is_error_detects_string_true_defensively() {
        assert!(driver_result_is_error(&json!({ "isError": "true" })));
    }

    #[test]
    fn driver_result_is_error_false_when_absent_or_false() {
        assert!(!driver_result_is_error(&json!({ "response": "ok" })));
        assert!(!driver_result_is_error(&json!({ "isError": false })));
    }

    #[test]
    fn driver_error_message_prefers_response_field() {
        let result = json!({ "isError": true, "response": "Timed out waiting for X" });
        let msg = driver_error_message("waitFor", &result);
        assert!(msg.contains("waitFor"));
        assert!(msg.contains("Timed out waiting for X"));
    }

    #[test]
    fn driver_error_message_falls_back_to_raw_json_without_response() {
        let result = json!({ "isError": true });
        let msg = driver_error_message("tap", &result);
        assert!(msg.contains("isError"));
    }

    #[test]
    fn driver_error_message_adds_hint_on_timeout_exception() {
        let result = json!({
            "isError": true,
            "response": "Timeout while executing tap: TimeoutException after 0:00:05.000000: Future not completed"
        });
        let msg = driver_error_message("tap", &result);
        assert!(msg.contains("flutter_snapshot"));
        assert!(msg.contains("by: \"key\""));
    }

    #[test]
    fn driver_error_message_no_hint_without_timeout_exception() {
        let result = json!({ "isError": true, "response": "Finder ambiguo: 2 matches" });
        let msg = driver_error_message("tap", &result);
        assert!(!msg.contains("flutter_snapshot"));
    }
}
