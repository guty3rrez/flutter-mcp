use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::application::error::{ApplicationError, Result};
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{Finder, Gesture};

type WsSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsReceiver = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Adaptador secundario que implementa FlutterVmPort comunicándose vía WebSocket JSON-RPC con la Dart VM
pub struct WebSocketVmServiceAdapter {
    connected: AtomicBool,
    request_counter: AtomicU64,
    sender: Arc<Mutex<Option<WsSender>>>,
    receiver: Arc<Mutex<Option<WsReceiver>>>,
    main_isolate_id: Arc<Mutex<Option<String>>>,
}

impl WebSocketVmServiceAdapter {
    pub fn new() -> Self {
        Self {
            connected: AtomicBool::new(false),
            request_counter: AtomicU64::new(1),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            main_isolate_id: Arc::new(Mutex::new(None)),
        }
    }

    async fn send_rpc(&self, method: &str, params: Value) -> Result<Value> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }

        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id.to_string(),
            "method": method,
            "params": params
        });

        let mut sender_guard = self.sender.lock().await;
        let sender = sender_guard
            .as_mut()
            .ok_or(ApplicationError::NotConnected)?;

        sender
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|e| ApplicationError::ConnectionError(format!("Error enviando RPC: {e}")))?;

        // En un cliente de producción completo se multiplexan los IDs; aquí leemos la siguiente respuesta
        let mut receiver_guard = self.receiver.lock().await;
        let receiver = receiver_guard
            .as_mut()
            .ok_or(ApplicationError::NotConnected)?;

        while let Some(msg_result) = receiver.next().await {
            let msg = msg_result.map_err(|e| {
                ApplicationError::ConnectionError(format!("Error en stream WebSocket: {e}"))
            })?;
            if let Message::Text(text) = msg {
                let parsed: Value = serde_json::from_str(&text)
                    .map_err(|e| ApplicationError::SerializationError(e.to_string()))?;

                // Validar si corresponde al ID enviado
                if parsed.get("id").and_then(|i| i.as_str()) == Some(&id.to_string()) {
                    if let Some(error) = parsed.get("error") {
                        return Err(ApplicationError::DriverError(error.to_string()));
                    }
                    return Ok(parsed.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        }

        Err(ApplicationError::ConnectionError(
            "Conexión cerrada antes de recibir respuesta RPC".into(),
        ))
    }
}

impl Default for WebSocketVmServiceAdapter {
    fn default() -> Self {
        Self::new()
    }
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
        *self.sender.lock().await = Some(s);
        *self.receiver.lock().await = Some(r);
        self.connected.store(true, Ordering::SeqCst);

        // Autodescubrir Isolate
        let vm_info = self.send_rpc("getVM", json!({})).await?;
        let first_isolate = vm_info
            .get("isolates")
            .and_then(|i| i.as_array())
            .and_then(|isolates| isolates.first())
            .and_then(|iso| iso.get("id"))
            .and_then(|id| id.as_str());
        if let Some(id) = first_isolate {
            *self.main_isolate_id.lock().await = Some(id.to_string());
        }

        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        let mut sender = self.sender.lock().await;
        if let Some(mut s) = sender.take() {
            let _ = s.close().await;
        }
        *self.receiver.lock().await = None;
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn get_diagnostics_tree(&self, subtree_depth: u32) -> Result<Value> {
        let isolate_id = self
            .main_isolate_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| "isolates/main".into());

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
        let isolate_id = self
            .main_isolate_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| "isolates/main".into());

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
        let isolate_id = self
            .main_isolate_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| "isolates/main".into());
        self.send_rpc("reloadSources", json!({ "isolateId": isolate_id }))
            .await?;
        Ok(())
    }

    async fn trigger_hot_restart(&self) -> Result<()> {
        let isolate_id = self
            .main_isolate_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| "isolates/main".into());

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
}
