use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;

pub struct FakeVmServer {
    pub uri: String,
    pub dispatched_commands: Arc<Mutex<Vec<String>>>,
    pub dispatched_params: Arc<Mutex<Vec<Value>>>,
    pub fail_commands: Arc<Mutex<HashMap<String, String>>>,
    stream_sender: mpsc::UnboundedSender<Value>,
}

impl FakeVmServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let dispatched_commands: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let dispatched_params: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let fail_commands: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<Value>();

        let commands_clone = dispatched_commands.clone();
        let params_clone = dispatched_params.clone();
        let fail_clone = fail_commands.clone();

        tokio::spawn(async move {
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => return,
            };
            let ws = match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            let (mut sink, mut source) = ws.split();

            // Task para transmitir streamNotify generados asíncronamente
            let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
            let sink_task = tokio::spawn(async move {
                while let Some(msg) = out_rx.recv().await {
                    if sink.send(msg).await.is_err() {
                        break;
                    }
                }
            });

            // Reenviar streamNotify al canal de salida
            let out_tx_stream = out_tx.clone();
            let stream_forward_task = tokio::spawn(async move {
                while let Some(event) = stream_rx.recv().await {
                    let text = event.to_string();
                    if out_tx_stream.send(Message::Text(text.into())).is_err() {
                        break;
                    }
                }
            });

            while let Some(Ok(Message::Text(text))) = source.next().await {
                let Ok(request) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let id = request.get("id");
                let method = request["method"].as_str().unwrap_or_default();
                let params = request.get("params").cloned().unwrap_or(json!({}));

                let response_result =
                    handle_rpc(method, &params, &commands_clone, &params_clone, &fail_clone).await;

                if let Some(id_val) = id {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id_val,
                        "result": response_result
                    });
                    if out_tx
                        .send(Message::Text(response.to_string().into()))
                        .is_err()
                    {
                        break;
                    }
                }
            }

            stream_forward_task.abort();
            sink_task.abort();
        });

        Self {
            uri: format!("ws://{addr}/ws"),
            dispatched_commands,
            dispatched_params,
            fail_commands,
            stream_sender: stream_tx,
        }
    }

    pub fn push_stdout_log(&self, line: &str) {
        use base64::prelude::*;
        let encoded = BASE64_STANDARD.encode(format!("{line}\n").as_bytes());
        let event = json!({
            "method": "streamNotify",
            "params": {
                "streamId": "Stdout",
                "event": {
                    "kind": "Stdout",
                    "timestamp": 1000,
                    "bytes": encoded
                }
            }
        });
        let _ = self.stream_sender.send(event);
    }

    pub fn push_stderr_error(&self, block: &str) {
        use base64::prelude::*;
        let encoded = BASE64_STANDARD.encode(block.as_bytes());
        let event = json!({
            "method": "streamNotify",
            "params": {
                "streamId": "Stderr",
                "event": {
                    "kind": "Stderr",
                    "timestamp": 2000,
                    "bytes": encoded
                }
            }
        });
        let _ = self.stream_sender.send(event);
    }
}

async fn handle_rpc(
    method: &str,
    params: &Value,
    commands: &Arc<Mutex<Vec<String>>>,
    params_log: &Arc<Mutex<Vec<Value>>>,
    fail_commands: &Arc<Mutex<HashMap<String, String>>>,
) -> Value {
    match method {
        "getVM" => json!({
            "isolates": [
                { "id": "isolates/1", "name": "main" }
            ]
        }),
        "streamListen" => json!({ "type": "Success" }),
        "setVMTimelineFlags" => json!({ "type": "Success" }),
        "reloadSources" => json!({ "type": "ReloadReport", "success": true }),
        "ext.flutter.reassemble" => json!({ "type": "Success" }),
        "hotRestart" => json!({ "type": "Success" }),
        "setExceptionPauseMode" => json!({ "type": "Success" }),
        "getVMTimeline" => json!({
            "traceEvents": [
                { "name": "Animator::BeginFrame", "ph": "B", "ts": 1000 },
                { "name": "Animator::BeginFrame", "ph": "E", "ts": 15000 },
                { "name": "Rasterizer::DrawToSurfaces", "ph": "B", "ts": 15000 },
                { "name": "Rasterizer::DrawToSurfaces", "ph": "E", "ts": 25000 }
            ]
        }),
        "ext.flutter.inspector.getRootWidgetTree" => realistic_flutter_tree(),
        "ext.flutter.inspector.getRootWidgetSummaryTree" => realistic_flutter_tree(),
        "ext.flutter.driver" => {
            let command = params
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or_default()
                .to_string();

            commands.lock().await.push(command.clone());
            params_log.lock().await.push(params.clone());

            if let Some(err_msg) = fail_commands.lock().await.get(&command) {
                return json!({
                    "isError": true,
                    "response": err_msg,
                    "type": "_extensionType"
                });
            }

            match command.as_str() {
                "screenshot" => {
                    // PNG 1x1 píxel transparente válido
                    const PNG_1X1_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
                    json!({
                        "isError": false,
                        "response": {
                            "data": PNG_1X1_BASE64
                        },
                        "type": "_extensionType"
                    })
                }
                "get_text" => json!({
                    "isError": false,
                    "response": {
                        "text": "Auralis Music Player"
                    },
                    "type": "_extensionType"
                }),
                _ => json!({
                    "isError": false,
                    "response": "{}",
                    "type": "_extensionType"
                }),
            }
        }
        _ => json!({ "type": "Success" }),
    }
}

fn realistic_flutter_tree() -> Value {
    json!({
        "description": "Scaffold",
        "children": [
            {
                "description": "AppBar",
                "children": [
                    {
                        "description": "Text",
                        "textPreview": "Auralis Music Player"
                    }
                ]
            },
            {
                "description": "Padding",
                "children": [
                    {
                        "description": "TextField",
                        "textPreview": "Search songs...",
                        "properties": [
                            { "name": "key", "description": "[<'search_input'>]" }
                        ]
                    }
                ]
            },
            {
                "description": "ElevatedButton",
                "textPreview": "Play",
                "properties": [
                    { "name": "key", "description": "[<'btn_play'>]" }
                ],
                "children": [
                    { "description": "Text", "textPreview": "Play" }
                ]
            },
            {
                "description": "ListView",
                "properties": [
                    { "name": "key", "description": "[<'track_list'>]" }
                ],
                "children": [
                    {
                        "description": "ListTile",
                        "textPreview": "Daft Punk - One More Time",
                        "properties": [
                            { "name": "key", "description": "[<'track_1'>]" }
                        ]
                    },
                    {
                        "description": "ListTile",
                        "textPreview": "Daft Punk - Aerodynamic",
                        "properties": [
                            { "name": "key", "description": "[<'track_2'>]" }
                        ]
                    }
                ]
            },
            {
                "description": "Tooltip",
                "properties": [
                    { "name": "message", "description": "Back" }
                ]
            },
            {
                "description": "CircularProgressIndicator",
                "properties": [
                    { "name": "key", "description": "[<'loading_spinner'>]" }
                ]
            }
        ]
    })
}
