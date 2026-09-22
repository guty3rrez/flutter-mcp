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
type ParamsLog = Arc<Mutex<Vec<Value>>>;

/// Levanta un servidor WebSocket local de un solo uso que responde a cada RPC JSON-RPC 2.0
/// entrante con lo que devuelva `responder(method, params)`, y registra el `command` de cada
/// llamada a `ext.flutter.driver` recibida (en orden) para poder aserirlo desde el test.
async fn spawn_fake_vm_service<F>(responder: F) -> (String, CommandLog)
where
    F: Fn(&str, &Value) -> Value + Send + Sync + 'static,
{
    let (uri, commands, _params) = spawn_fake_vm_service_with_params(responder).await;
    (uri, commands)
}

/// Igual que `spawn_fake_vm_service` pero además registra, en el mismo orden, los `params`
/// completos de cada llamada a `ext.flutter.driver` -- necesario para aserir el valor real de
/// campos como `timeout` (no solo qué comando se invocó).
async fn spawn_fake_vm_service_with_params<F>(responder: F) -> (String, CommandLog, ParamsLog)
where
    F: Fn(&str, &Value) -> Value + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let commands: CommandLog = Arc::new(Mutex::new(Vec::new()));
    let commands_clone = commands.clone();
    let params_log: ParamsLog = Arc::new(Mutex::new(Vec::new()));
    let params_log_clone = params_log.clone();

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
                params_log_clone.lock().await.push(params.clone());
            }

            let result = responder(&method, &params);
            let response = json!({ "jsonrpc": "2.0", "id": request["id"], "result": result });
            let _ = sink.send(Message::Text(response.to_string().into())).await;
        }
    });

    (format!("ws://{addr}/ws"), commands, params_log)
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

#[tokio::test]
async fn wait_for_sends_timeout_ms_unconverted() {
    let (uri, _commands, params) = spawn_fake_vm_service_with_params(|method, _| {
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
        .wait_for(&Finder::by_text("Hola", true), 2000)
        .await
        .unwrap();

    let logged = params.lock().await;
    let wait_for_call = logged
        .iter()
        .find(|p| p.get("command").and_then(|c| c.as_str()) == Some("waitFor"))
        .expect("debe haber invocado waitFor");
    assert_eq!(
        wait_for_call.get("timeout").and_then(|t| t.as_str()),
        Some("2000")
    );
}

#[tokio::test]
async fn wait_for_absent_sends_timeout_ms_unconverted() {
    let (uri, _commands, params) = spawn_fake_vm_service_with_params(|method, _| {
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
        .wait_for_absent(&Finder::by_text("Hola", true), 3000)
        .await
        .unwrap();

    let logged = params.lock().await;
    let call = logged
        .iter()
        .find(|p| p.get("command").and_then(|c| c.as_str()) == Some("waitForAbsent"))
        .expect("debe haber invocado waitForAbsent");
    assert_eq!(call.get("timeout").and_then(|t| t.as_str()), Some("3000"));
}

/// Árbol de diagnóstico fake con un único nodo de texto, en el shape que devuelve
/// `ext.flutter.inspector.getRootWidgetTree(withPreviews: true)` (la RPC real que usa
/// `get_diagnostics_tree`) y que `TreePruner` sabe podar.
fn tree_with_text(text: &str) -> Value {
    json!({
        "description": "Scaffold",
        "children": [
            {
                "description": "Text",
                "textPreview": text
            }
        ]
    })
}

#[tokio::test]
async fn tap_uses_fast_fail_timeout_when_precheck_finds_no_match() {
    let (uri, _commands, params) = spawn_fake_vm_service_with_params(|method, _| {
        if method == "getVM" {
            getvm_result()
        } else if method == "ext.flutter.inspector.getRootWidgetTree" {
            tree_with_text("Cancelar")
        } else {
            ok_driver_result()
        }
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    adapter
        .dispatch_gesture(&Gesture::Tap {
            finder: Finder::by_text("Texto Que No Existe", true),
        })
        .await
        .expect("el comando real igual se intenta, aunque el pre-chequeo no haya matcheado");

    let logged = params.lock().await;
    let tap_call = logged
        .iter()
        .find(|p| p.get("command").and_then(|c| c.as_str()) == Some("tap"))
        .expect("debe haber invocado tap");
    assert_eq!(
        tap_call.get("timeout").and_then(|t| t.as_str()),
        Some("800")
    );
}

#[tokio::test]
async fn tap_uses_default_timeout_when_precheck_finds_match() {
    let (uri, _commands, params) = spawn_fake_vm_service_with_params(|method, _| {
        if method == "getVM" {
            getvm_result()
        } else if method == "ext.flutter.inspector.getRootWidgetTree" {
            tree_with_text("Guardar")
        } else {
            ok_driver_result()
        }
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    adapter
        .dispatch_gesture(&Gesture::Tap {
            finder: Finder::by_text("Guardar", true),
        })
        .await
        .unwrap();

    let logged = params.lock().await;
    let tap_call = logged
        .iter()
        .find(|p| p.get("command").and_then(|c| c.as_str()) == Some("tap"))
        .expect("debe haber invocado tap");
    assert_eq!(
        tap_call.get("timeout").and_then(|t| t.as_str()),
        Some("5000")
    );
}

#[tokio::test]
async fn tap_error_includes_candidate_suggestions_when_precheck_finds_similar_text() {
    let (uri, _commands) = spawn_fake_vm_service(|method, params| {
        if method == "getVM" {
            return getvm_result();
        }
        if method == "ext.flutter.inspector.getRootWidgetTree" {
            return tree_with_text("Guardar cambios");
        }
        if method == "ext.flutter.driver"
            && params.get("command").and_then(|c| c.as_str()) == Some("tap")
        {
            return json!({
                "isError": true,
                "response": "Timeout while executing tap: TimeoutException after 0:00:00.800000: Future not completed"
            });
        }
        ok_driver_result()
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    let err = adapter
        .dispatch_gesture(&Gesture::Tap {
            finder: Finder::by_text("Guardar", true),
        })
        .await
        .expect_err("el tap real también falla en este escenario");

    assert!(err.to_string().contains("Guardar cambios"));
}

#[tokio::test]
async fn scroll_until_visible_returns_err_when_max_scrolls_exhausted() {
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
        .dispatch_gesture(&Gesture::ScrollUntilVisible {
            scrollable: None,
            target: Finder::by_text("Nunca Aparece", true),
            delta: -200.0,
            max_scrolls: 2,
        })
        .await
        .expect_err("agotar max_scrolls sin match debe devolver Err, no Ok silencioso");

    assert!(err.to_string().contains("scrollUntilVisible"));
}

/// Igual que `spawn_fake_vm_service` pero el `responder` puede devolver un error JSON-RPC
/// (`Err(mensaje)`) en vez de un `result` -- necesario para simular un SDK de Flutter viejo que
/// no reconoce `ext.flutter.inspector.getRootWidgetTree` y confirmar el fallback a
/// `getRootWidgetSummaryTree` en `get_diagnostics_tree`.
async fn spawn_fake_vm_service_fallible<F>(responder: F) -> String
where
    F: Fn(&str, &Value) -> Result<Value, String> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let (mut sink, mut source) = ws.split();

        while let Some(Ok(Message::Text(text))) = source.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let method = request["method"].as_str().unwrap_or_default().to_string();
            let params = request["params"].clone();

            let response = match responder(&method, &params) {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": request["id"], "result": result }),
                Err(message) => {
                    json!({ "jsonrpc": "2.0", "id": request["id"], "error": { "message": message } })
                }
            };
            let _ = sink.send(Message::Text(response.to_string().into())).await;
        }
    });

    format!("ws://{addr}/ws")
}

#[tokio::test]
async fn get_diagnostics_tree_falls_back_to_legacy_rpc_when_new_one_is_unsupported() {
    let uri = spawn_fake_vm_service_fallible(|method, _| match method {
        "getVM" => Ok(getvm_result()),
        "ext.flutter.inspector.getRootWidgetTree" => Err("Unknown service extension".to_string()),
        "ext.flutter.inspector.getRootWidgetSummaryTree" => Ok(json!({
            "description": "Scaffold",
            "properties": [{"name": "data", "description": "Desde legacy"}]
        })),
        _ => Ok(ok_driver_result()),
    })
    .await;

    let adapter = WebSocketVmServiceAdapter::new();
    adapter.connect(&uri).await.expect("debe conectar");

    let tree = adapter
        .get_diagnostics_tree(50)
        .await
        .expect("debe caer al fallback en vez de propagar el error");
    assert_eq!(tree["description"], "Scaffold");
}
