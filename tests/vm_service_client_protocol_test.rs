//! Pruebas a nivel de protocolo del adaptador REAL `WebSocketVmServiceAdapter` contra un
//! servidor WebSocket local que imita el JSON-RPC del Dart VM Service / Flutter Driver.
//! No reemplazan a `tests/integration_test.rs` (cableado hexagonal completo vía el fake
//! `MockVmServiceAdapter`); verifican específicamente la forma real del wire protocol -- el
//! shape de `isError`, la secuencia de comandos de `ensure_driver_extension_configured`, y el
//! foco antes de `enter_text` -- que ese fake, deliberadamente, no replica.

use flutter_mcp::{Finder, FlutterVmPort, Gesture, WebSocketVmServiceAdapter};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

type CommandLog = Arc<Mutex<Vec<String>>>;

/// Levanta un servidor WebSocket local de un solo uso que responde a cada RPC JSON-RPC 2.0
/// entrante con lo que devuelva `responder(method, params)`, y registra el `command` de cada
/// llamada a `ext.flutter.driver` recibida (en orden) para poder aserirlo desde el test.
async fn spawn_fake_vm_service<F>(responder: F) -> (String, CommandLog)
where
    F: Fn(&str, &Value) -> Value + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let commands: CommandLog = Arc::new(Mutex::new(Vec::new()));
    let commands_clone = commands.clone();

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let (mut sink, mut source) = ws.split();

        while let Some(Ok(Message::Text(text))) = source.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let method = request["method"].as_str().unwrap_or_default().to_string();
            let params = request["params"].clone();

            if method == "ext.flutter.driver"
                && let Some(command) = params.get("command").and_then(|c| c.as_str())
            {
                commands_clone.lock().await.push(command.to_string());
            }

            let result = responder(&method, &params);
            let response = json!({ "jsonrpc": "2.0", "id": request["id"], "result": result });
            let _ = sink.send(Message::Text(response.to_string().into())).await;
        }
    });

    (format!("ws://{addr}/ws"), commands)
}

fn ok_driver_result() -> Value {
    json!({ "isError": false, "response": "{}", "type": "_extensionType" })
}

fn getvm_result() -> Value {
    json!({ "isolates": [{ "id": "isolates/1" }] })
}

#[tokio::test]
async fn execute_driver_command_returns_err_when_driver_reports_is_error() {
    let (uri, _commands) = spawn_fake_vm_service(|method, params| {
        if method == "getVM" {
            return getvm_result();
        }
        if method == "ext.flutter.driver"
            && params.get("command").and_then(|c| c.as_str()) == Some("waitFor")
        {
            return json!({ "isError": true, "response": "Waited ... Timed out" });
        }
        ok_driver_result()
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    let err = adapter
        .execute_driver_command("waitFor", json!({}))
        .await
        .expect_err("isError:true debe mapear a Err");
    assert!(err.to_string().contains("Timed out"));
}

#[tokio::test]
async fn driver_extension_is_configured_once_and_reset_after_hot_restart() {
    let (uri, commands) = spawn_fake_vm_service(|method, _| {
        if method == "getVM" {
            getvm_result()
        } else {
            ok_driver_result()
        }
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    adapter
        .execute_driver_command("tap", json!({}))
        .await
        .unwrap();
    adapter
        .execute_driver_command("tap", json!({}))
        .await
        .unwrap();
    assert_eq!(
        *commands.lock().await,
        vec!["set_frame_sync", "set_text_entry_emulation", "tap", "tap"]
    );

    adapter.trigger_hot_restart().await.unwrap();
    commands.lock().await.clear();

    adapter
        .execute_driver_command("tap", json!({}))
        .await
        .unwrap();
    assert_eq!(
        *commands.lock().await,
        vec!["set_frame_sync", "set_text_entry_emulation", "tap"]
    );
}

#[tokio::test]
async fn enter_text_taps_the_finder_before_entering_text() {
    let (uri, commands) = spawn_fake_vm_service(|method, _| {
        if method == "getVM" {
            getvm_result()
        } else {
            ok_driver_result()
        }
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    adapter
        .dispatch_gesture(&Gesture::EnterText {
            finder: Finder::by_key("email_field"),
            text: "hi".into(),
        })
        .await
        .unwrap();

    assert_eq!(
        *commands.lock().await,
        vec![
            "set_frame_sync",
            "set_text_entry_emulation",
            "tap",
            "enter_text"
        ]
    );
}

#[tokio::test]
async fn clear_text_taps_the_finder_before_clearing_text() {
    let (uri, commands) = spawn_fake_vm_service(|method, _| {
        if method == "getVM" {
            getvm_result()
        } else {
            ok_driver_result()
        }
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    adapter
        .dispatch_gesture(&Gesture::ClearText {
            finder: Finder::by_key("email_field"),
        })
        .await
        .unwrap();

    assert_eq!(
        *commands.lock().await,
        vec![
            "set_frame_sync",
            "set_text_entry_emulation",
            "tap",
            "enter_text"
        ]
    );
}
