mod common;

use common::FakeVmServer;
use flutter_mcp::{
    ConnectParams, DriverRawParams, EnterTextParams, FlutterMcpServer, FlutterPopParams,
    GetErrorsParams, GetLogsParams, GetPerformanceParams, GetTextParams, LocalFileSystemAdapter,
    ScreenshotParams, ScrollIntoViewParams, ScrollParams, StartControlParams, TapParams,
    WaitForParams, WebSocketVmServiceAdapter,
};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn test_e2e_wire_all_19_tools_happy_path() {
    // 1. Levantar servidor wire de alta fidelidad que simula Flutter 3.47+
    let fake_vm = FakeVmServer::start().await;

    // 2. Crear entorno temporal con pubspec.yaml y lib/main.dart para probar start_control
    let temp_dir = tempfile::tempdir().expect("tempdir debe crearse");
    let project_root = temp_dir.path().to_str().unwrap().to_string();
    let pubspec_path = temp_dir.path().join("pubspec.yaml");
    let lib_dir = temp_dir.path().join("lib");
    tokio::fs::create_dir(&lib_dir).await.unwrap();
    let main_dart_path = lib_dir.join("main.dart");

    tokio::fs::write(
        &pubspec_path,
        "name: dummy_app\ndev_dependencies:\n  flutter_driver:\n    sdk: flutter\n",
    )
    .await
    .unwrap();
    tokio::fs::write(&main_dart_path, "void main() {\n  runApp(MyApp());\n}\n")
        .await
        .unwrap();

    // 3. Instanciar adaptadores reales y servidor MCP
    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    // ==========================================
    // TOOL 1: flutter_connect
    // ==========================================
    let res = server
        .flutter_connect(Parameters(ConnectParams {
            uri: fake_vm.uri.clone(),
        }))
        .await
        .expect("Debe conectar exitosamente");
    assert!(!res.is_error.unwrap_or(false));
    assert!(res.content[0].as_text().unwrap().text.contains("Conectado"));

    // ==========================================
    // TOOL 2: flutter_snapshot
    // ==========================================
    let res = server
        .flutter_snapshot()
        .await
        .expect("Debe obtener snapshot podado");
    assert!(!res.is_error.unwrap_or(false));
    let snapshot_json: serde_json::Value =
        serde_json::from_str(&res.content[0].as_text().unwrap().text)
            .expect("El snapshot debe ser JSON válido");
    assert_eq!(snapshot_json["widget_type"], "Scaffold");
    // Verificar que extrajo textPreview y podó Padding
    let children = snapshot_json["children"].as_array().unwrap();
    assert!(
        children
            .iter()
            .any(|c| c["widget_type"] == "TextField" && c["text"] == "Search songs...")
    );
    assert!(
        children
            .iter()
            .any(|c| c["widget_type"] == "ElevatedButton" && c["is_interactive"] == true)
    );

    // ==========================================
    // TOOL 3: flutter_tap
    // ==========================================
    let res = server
        .flutter_tap(Parameters(TapParams {
            by: "key".into(),
            value: "btn_play".into(),
            timeout_ms: None,
        }))
        .await
        .expect("Debe despachar tap");
    assert!(!res.is_error.unwrap_or(false));
    // Validar que se enviaron automáticamente set_frame_sync y set_text_entry_emulation antes del
    // tap. "request_data" es el sondeo (una sola vez, cacheado) del handler custom de Fase B
    // disparado por el flutter_snapshot de TOOL 2 -- este fake server no lo registra, así que
    // get_diagnostics_tree cae al árbol legacy, tal como espera el resto de este test.
    {
        let commands = fake_vm.dispatched_commands.lock().await;
        assert_eq!(
            *commands,
            vec![
                "request_data",
                "set_frame_sync",
                "set_text_entry_emulation",
                "tap"
            ]
        );
    }

    // ==========================================
    // TOOL 4: flutter_pop (PageBack)
    // ==========================================
    let res = server
        .flutter_pop(Parameters(FlutterPopParams { timeout_ms: None }))
        .await
        .expect("Debe despachar pop");
    assert!(!res.is_error.unwrap_or(false));
    {
        let params_log = fake_vm.dispatched_params.lock().await;
        let last_call = params_log.last().unwrap();
        assert_eq!(last_call["command"], "tap");
        assert_eq!(last_call["finderType"], "PageBack");
    }

    // ==========================================
    // TOOL 5: flutter_enter_text
    // ==========================================
    let res = server
        .flutter_enter_text(Parameters(EnterTextParams {
            by: "key".into(),
            value: "search_input".into(),
            text: "Daft Punk".into(),
            timeout_ms: None,
        }))
        .await
        .expect("Debe despachar enter_text");
    assert!(!res.is_error.unwrap_or(false));
    // Comprobar que hizo tap de foco previo y luego enter_text
    {
        let commands = fake_vm.dispatched_commands.lock().await;
        let len = commands.len();
        assert_eq!(commands[len - 2], "tap");
        assert_eq!(commands[len - 1], "enter_text");
    }

    // ==========================================
    // TOOL 6: flutter_get_text
    // ==========================================
    let res = server
        .flutter_get_text(Parameters(GetTextParams {
            by: "key".into(),
            value: "btn_play".into(),
        }))
        .await
        .expect("Debe obtener texto");
    assert!(!res.is_error.unwrap_or(false));
    assert_eq!(
        res.content[0].as_text().unwrap().text,
        "Auralis Music Player"
    );

    // ==========================================
    // TOOL 7: flutter_scroll
    // ==========================================
    let res = server
        .flutter_scroll(Parameters(ScrollParams {
            by: "key".into(),
            value: "track_list".into(),
            dx: 0.0,
            dy: -200.0,
            duration_ms: Some(250),
            frequency: Some(60),
            timeout_ms: None,
        }))
        .await
        .expect("Debe despachar scroll");
    assert!(!res.is_error.unwrap_or(false));
    {
        let params_log = fake_vm.dispatched_params.lock().await;
        let last_call = params_log.last().unwrap();
        assert_eq!(last_call["command"], "scroll");
        assert_eq!(last_call["dy"], "-200");
    }

    // ==========================================
    // TOOL 8: flutter_scroll_into_view
    // ==========================================
    let res = server
        .flutter_scroll_into_view(Parameters(ScrollIntoViewParams {
            by: "key".into(),
            value: "track_2".into(),
            alignment: Some(0.5),
            timeout_ms: None,
        }))
        .await
        .expect("Debe despachar scroll_into_view");
    assert!(!res.is_error.unwrap_or(false));
    {
        let params_log = fake_vm.dispatched_params.lock().await;
        let last_call = params_log.last().unwrap();
        assert_eq!(last_call["command"], "scrollIntoView");
        assert_eq!(last_call["alignment"], "0.5");
    }

    // ==========================================
    // TOOL 9: flutter_wait_for
    // ==========================================
    let res = server
        .flutter_wait_for(Parameters(WaitForParams {
            by: "key".into(),
            value: "btn_play".into(),
            timeout_ms: Some(3000),
        }))
        .await
        .expect("Debe despachar waitFor");
    assert!(!res.is_error.unwrap_or(false));

    // ==========================================
    // TOOL 10: flutter_wait_for_absent
    // ==========================================
    let res = server
        .flutter_wait_for_absent(Parameters(WaitForParams {
            by: "key".into(),
            value: "loading_spinner".into(),
            timeout_ms: Some(3000),
        }))
        .await
        .expect("Debe despachar waitForAbsent");
    assert!(!res.is_error.unwrap_or(false));

    // ==========================================
    // TOOL 11: flutter_screenshot
    // ==========================================
    let screenshot_file = temp_dir.path().join("screenshot.png");
    let res = server
        .flutter_screenshot(Parameters(ScreenshotParams {
            save_path: Some(screenshot_file.to_str().unwrap().to_string()),
        }))
        .await
        .expect("Debe tomar screenshot");
    assert!(!res.is_error.unwrap_or(false));
    assert!(screenshot_file.exists());
    let png_bytes = tokio::fs::read(&screenshot_file).await.unwrap();
    // Validar cabecera mágica de archivo PNG
    assert_eq!(&png_bytes[0..8], b"\x89PNG\r\n\x1a\n");

    // ==========================================
    // TOOL 12: flutter_hot_reload
    // ==========================================
    let res = server
        .flutter_hot_reload()
        .await
        .expect("Debe ejecutar hot reload");
    assert!(!res.is_error.unwrap_or(false));

    // ==========================================
    // TOOL 13: flutter_hot_restart
    // ==========================================
    let res = server
        .flutter_hot_restart()
        .await
        .expect("Debe ejecutar hot restart");
    assert!(!res.is_error.unwrap_or(false));
    // Comprobar que tras el restart, el siguiente tap vuelve a configurar frame_sync y text_entry_emulation
    fake_vm.dispatched_commands.lock().await.clear();
    server
        .flutter_tap(Parameters(TapParams {
            by: "key".into(),
            value: "btn_play".into(),
            timeout_ms: None,
        }))
        .await
        .unwrap();
    {
        let commands = fake_vm.dispatched_commands.lock().await;
        assert_eq!(
            *commands,
            vec!["set_frame_sync", "set_text_entry_emulation", "tap"]
        );
    }

    // ==========================================
    // TOOL 14: flutter_driver_raw
    // ==========================================
    let res = server
        .flutter_driver_raw(Parameters(DriverRawParams {
            command: "custom_ping".into(),
            params: json!({ "custom_key": "val" }),
        }))
        .await
        .expect("Debe ejecutar driver_raw");
    assert!(!res.is_error.unwrap_or(false));

    // ==========================================
    // TOOL 15: flutter_get_logs
    // ==========================================
    fake_vm.push_stdout_log("Auralis audio session initialized");
    fake_vm.push_stdout_log("Playing track 1");
    // Pequeño sleep para que el task lector de websocket procese los streamNotify
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let res = server
        .flutter_get_logs(Parameters(GetLogsParams {
            filter: Some("track".into()),
            source: Some("stdout".into()),
            limit: Some(10),
        }))
        .await
        .expect("Debe leer logs");
    assert!(!res.is_error.unwrap_or(false));
    let logs_text = &res.content[0].as_text().unwrap().text;
    assert!(logs_text.contains("Playing track 1"));
    assert!(!logs_text.contains("audio session initialized"));

    // ==========================================
    // TOOL 16: flutter_get_errors
    // ==========================================
    fake_vm.push_stderr_error(
        "══╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞══════════════════════\n\
         The following assertion was thrown building PlayerScreen:\n\
         RenderFlex overflowed by 22 pixels on the bottom.\n\
         ═══════════════════════════════════════════════════════════════\n",
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let res = server
        .flutter_get_errors(Parameters(GetErrorsParams {
            limit: Some(10),
            precise: Some(false),
        }))
        .await
        .expect("Debe leer errores");
    assert!(!res.is_error.unwrap_or(false));
    let errors_text = &res.content[0].as_text().unwrap().text;
    assert!(errors_text.contains("RenderFlex overflowed by 22 pixels"));

    // ==========================================
    // TOOL 17: flutter_get_performance
    // ==========================================
    let res = server
        .flutter_get_performance(Parameters(GetPerformanceParams {
            window_ms: None,
            include_frames: Some(true),
        }))
        .await
        .expect("Debe leer reporte de rendimiento");
    assert!(!res.is_error.unwrap_or(false));
    let perf_text = &res.content[0].as_text().unwrap().text;
    assert!(perf_text.contains("Frames analizados: 1"));
    assert!(perf_text.contains("Frames con jank: 1"));

    // ==========================================
    // TOOL 18: flutter_start_control
    // ==========================================
    let res = server
        .flutter_start_control(Parameters(StartControlParams {
            project_root: Some(project_root.clone()),
            entrypoint: Some("lib/main.dart".into()),
            revert_after_restart: Some(true),
        }))
        .await
        .expect("Debe inyectar control y revertir");
    assert!(!res.is_error.unwrap_or(false));
    assert!(res.content[0].as_text().unwrap().text.contains("revertido"));
    // Verificar que el archivo en disco quedó intacto tras revert
    let main_content = tokio::fs::read_to_string(&main_dart_path).await.unwrap();
    assert!(!main_content.contains("enableFlutterDriverExtension"));

    // ==========================================
    // TOOL 19: flutter_disconnect
    // ==========================================
    let res = server
        .flutter_disconnect()
        .await
        .expect("Debe desconectar exitosamente");
    assert!(!res.is_error.unwrap_or(false));
    assert!(
        res.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("Desconectado")
    );
}

#[tokio::test]
async fn test_e2e_wire_driver_is_error_reported_as_mcp_error() {
    let fake_vm = FakeVmServer::start().await;
    fake_vm.fail_commands.lock().await.insert(
        "tap".into(),
        "Timeout while executing tap: TimeoutException after 0:00:05: Future not completed".into(),
    );

    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    server
        .flutter_connect(Parameters(ConnectParams {
            uri: fake_vm.uri.clone(),
        }))
        .await
        .unwrap();

    let res = server
        .flutter_tap(Parameters(TapParams {
            by: "key".into(),
            value: "boton_inexistente".into(),
            timeout_ms: None,
        }))
        .await
        .expect("La llamada a tool debe retornar CallToolResult");

    // IMPORTANTE: isError: true en Flutter Driver debe mapearse a is_error: true en MCP
    assert!(res.is_error.unwrap_or(false));
    let err_msg = &res.content[0].as_text().unwrap().text;
    assert!(err_msg.contains("Timeout while executing tap"));
    assert!(err_msg.contains("flutter_snapshot"));
}

#[tokio::test]
async fn test_e2e_wire_candidate_suggestions_on_text_mismatch() {
    let fake_vm = FakeVmServer::start().await;
    fake_vm.fail_commands.lock().await.insert(
        "tap".into(),
        "Timeout while executing tap: TimeoutException after 0:00:00.800000: Future not completed"
            .into(),
    );

    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    server
        .flutter_connect(Parameters(ConnectParams {
            uri: fake_vm.uri.clone(),
        }))
        .await
        .unwrap();

    // El árbol fake contiene "Auralis Music Player". Buscamos "Auralis Music" (similar)
    let res = server
        .flutter_tap(Parameters(TapParams {
            by: "text".into(),
            value: "Auralis Music".into(),
            timeout_ms: None,
        }))
        .await
        .unwrap();

    assert!(res.is_error.unwrap_or(false));
    let err_msg = &res.content[0].as_text().unwrap().text;
    // Debe incluir la sugerencia de candidatos similares del árbol
    assert!(err_msg.contains("Auralis Music Player"));
}

#[tokio::test]
async fn test_e2e_wire_explicit_timeout_override_skips_precheck() {
    let fake_vm = FakeVmServer::start().await;

    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    server
        .flutter_connect(Parameters(ConnectParams {
            uri: fake_vm.uri.clone(),
        }))
        .await
        .unwrap();

    server
        .flutter_tap(Parameters(TapParams {
            by: "text".into(),
            value: "Cualquier Cosa".into(),
            timeout_ms: Some(1750),
        }))
        .await
        .unwrap();

    let params_log = fake_vm.dispatched_params.lock().await;
    let tap_call = params_log
        .iter()
        .find(|p| p["command"] == "tap")
        .expect("debe haber llamado tap");
    assert_eq!(tap_call["timeout"], "1750");
}

#[tokio::test]
async fn test_e2e_wire_mcp_raw_json_dispatch_and_schema_validation() {
    let fake_vm = FakeVmServer::start().await;

    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    // 1. Dispatch flutter_connect con JSON sin tipado estático
    let res = server
        .dispatch_tool("flutter_connect", json!({ "uri": fake_vm.uri }))
        .await
        .expect("dispatch_tool debe funcionar con JSON crudo");
    assert!(!res.is_error.unwrap_or(false));

    // 2. Dispatch flutter_tap con JSON crudo
    let res = server
        .dispatch_tool(
            "flutter_tap",
            json!({ "by": "key", "value": "btn_play", "timeout_ms": 2500 }),
        )
        .await
        .expect("dispatch_tool de tap debe funcionar");
    assert!(!res.is_error.unwrap_or(false));

    // 3. Dispatch de herramienta inexistente devuelve Err
    let err = server
        .dispatch_tool("herramienta_fantasma", json!({}))
        .await
        .expect_err("Herramienta desconocida debe retornar Err");
    assert!(err.contains("Herramienta desconocida"));

    // 4. Parámetros inválidos devuelven Err de deserialización
    let err = server
        .dispatch_tool("flutter_tap", json!({ "campo_invalido": 123 }))
        .await
        .expect_err("Parámetros inválidos deben fallar deserialización");
    assert!(err.contains("missing field") || err.contains("by"));
}
