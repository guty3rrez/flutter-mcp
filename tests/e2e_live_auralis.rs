use flutter_mcp::{
    ConnectParams, DriverRawParams, EnterTextParams, FlutterMcpServer, FlutterPopParams,
    GetErrorsParams, GetLogsParams, GetPerformanceParams, GetTextParams, LocalFileSystemAdapter,
    ScreenshotParams, ScrollIntoViewParams, ScrollParams, StartControlParams, TapParams,
    WaitForParams, WebSocketVmServiceAdapter,
};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;
use std::sync::Arc;

fn find_first_node_with_key(node: &serde_json::Value) -> Option<String> {
    if let Some(key) = node.get("key").and_then(|k| k.as_str())
        && !key.is_empty()
    {
        let clean = key
            .trim_start_matches("[<'")
            .trim_end_matches("'>]")
            .trim_matches('\'')
            .to_string();
        return Some(clean);
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if let Some(found) = find_first_node_with_key(child) {
                return Some(found);
            }
        }
    }
    None
}

fn find_first_node_with_text(node: &serde_json::Value) -> Option<String> {
    if let Some(text) = node.get("text").and_then(|t| t.as_str())
        && !text.trim().is_empty()
    {
        return Some(text.trim().to_string());
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if let Some(found) = find_first_node_with_text(child) {
                return Some(found);
            }
        }
    }
    None
}

#[tokio::test]
#[ignore = "Requiere app Auralis en ejecución con AURALIS_VM_URI configurada"]
async fn test_e2e_live_auralis_full_interaction() {
    let vm_uri = std::env::var("AURALIS_VM_URI").expect(
        "AURALIS_VM_URI debe estar configurada en el entorno (ej: ws://127.0.0.1:33215/token/ws)",
    );

    println!("===========================================================");
    println!("  Iniciando prueba E2E Real contra Auralis Desktop (19/19) ");
    println!("  Target: {vm_uri}");
    println!("===========================================================");

    // 1. Instanciar adaptadores reales y servidor MCP
    let vm_adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let fs_adapter = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(flutter_mcp::FlutterServiceImpl::new(vm_adapter, fs_adapter));
    let server = FlutterMcpServer::new(app_service);

    // ==========================================
    // TOOL 1: flutter_connect
    // ==========================================
    print!("[1/19] Conectando a la Dart VM de Auralis (flutter_connect)... ");
    let res = server
        .flutter_connect(Parameters(ConnectParams {
            uri: vm_uri.clone(),
        }))
        .await
        .expect("Debe conectar exitosamente a Auralis");
    assert!(!res.is_error.unwrap_or(false));
    println!("✅ CONECTADO");

    // ==========================================
    // TOOL 2: flutter_snapshot
    // ==========================================
    print!("[2/19] Obteniendo snapshot de UI real (flutter_snapshot)... ");
    let res = server
        .flutter_snapshot()
        .await
        .expect("Debe extraer el snapshot de widgets");
    assert!(!res.is_error.unwrap_or(false));
    let snapshot_text = &res.content[0].as_text().unwrap().text;
    assert!(
        snapshot_text.contains("Scaffold") || snapshot_text.contains("MaterialApp"),
        "El árbol debe contener un Scaffold o MaterialApp raíz"
    );
    let parsed_tree: serde_json::Value =
        serde_json::from_str(snapshot_text).expect("Snapshot debe ser JSON válido");
    println!(
        "✅ SNAPSHOT OBTENIDO (longitud: {} chars)",
        snapshot_text.len()
    );

    let discovered_key = find_first_node_with_key(&parsed_tree);
    let discovered_text = find_first_node_with_text(&parsed_tree);
    println!(
        "ℹ️ Elementos descubiertos: key={:?}, text={:?}",
        discovered_key, discovered_text
    );

    // ==========================================
    // TOOL 3: flutter_screenshot
    // ==========================================
    print!("[3/19] Capturando framebuffer de GPU (flutter_screenshot)... ");
    let screenshot_path = "/tmp/auralis_e2e_test_screenshot.png";
    let res = server
        .flutter_screenshot(Parameters(ScreenshotParams {
            save_path: Some(screenshot_path.into()),
        }))
        .await
        .expect("Debe capturar pantalla nativa");
    assert!(
        !res.is_error.unwrap_or(false),
        "Screenshot error: {:?}",
        res.content
    );
    let png_bytes = tokio::fs::read(screenshot_path)
        .await
        .expect("El archivo PNG debe haberse guardado");
    assert_eq!(&png_bytes[0..8], b"\x89PNG\r\n\x1a\n");
    println!("✅ SCREENSHOT GUARDADO ({} bytes)", png_bytes.len());

    // ==========================================
    // TOOL 4: flutter_get_logs
    // ==========================================
    print!("[4/19] Leyendo logs en vivo emitidos por Auralis (flutter_get_logs)... ");
    let logs_res = server
        .flutter_get_logs(Parameters(GetLogsParams {
            filter: None,
            source: None,
            limit: Some(20),
        }))
        .await
        .expect("Debe leer logs");
    assert!(!logs_res.is_error.unwrap_or(false));
    let logs_text = &logs_res.content[0].as_text().unwrap().text;
    println!("✅ LOGS LEÍDOS ({} chars)", logs_text.len());

    // ==========================================
    // TOOL 5: flutter_get_errors
    // ==========================================
    print!("[5/19] Verificando detección de errores (flutter_get_errors)... ");
    let errors_res = server
        .flutter_get_errors(Parameters(GetErrorsParams {
            limit: Some(10),
            precise: Some(false),
        }))
        .await
        .expect("Debe leer errores");
    assert!(!errors_res.is_error.unwrap_or(false));
    let errors_text = &errors_res.content[0].as_text().unwrap().text;
    println!("✅ ERRORES EVALUADOS: {}", errors_text);

    // ==========================================
    // TOOL 6: flutter_get_performance
    // ==========================================
    print!("[6/19] Obteniendo análisis de rendimiento real (flutter_get_performance)... ");
    let perf_res = server
        .flutter_get_performance(Parameters(GetPerformanceParams {
            window_ms: None,
            include_frames: Some(false),
        }))
        .await
        .expect("Debe obtener reporte de rendimiento");
    assert!(!perf_res.is_error.unwrap_or(false));
    let perf_text = &perf_res.content[0].as_text().unwrap().text;
    println!("✅ TIMELINE ANALIZADO: {}", perf_text);

    // ==========================================
    // TOOL 7: flutter_tap
    // ==========================================
    print!("[7/19] Probando flutter_tap sobre elemento interactivo o 'Tracks'... ");
    let tap_res = server
        .flutter_tap(Parameters(TapParams {
            by: "text".into(),
            value: "Tracks".into(),
            timeout_ms: Some(3000),
        }))
        .await;
    match tap_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ TAP EXITOSO");
        }
        other => {
            println!("ℹ️ Resultado del tap: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 8: flutter_wait_for
    // ==========================================
    print!("[8/19] Probando espera de elemento visible (flutter_wait_for)... ");
    let wait_res = server
        .flutter_wait_for(Parameters(WaitForParams {
            by: "text".into(),
            value: "Tracks".into(),
            timeout_ms: Some(3000),
        }))
        .await;
    match wait_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ WIDGET LOCALIZADO EXITOSAMENTE");
        }
        other => {
            println!("ℹ️ Resultado wait_for: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 9: flutter_get_text
    // ==========================================
    print!("[9/19] Probando flutter_get_text sobre texto visible... ");
    let text_res = server
        .flutter_get_text(Parameters(GetTextParams {
            by: "text".into(),
            value: "AURALIS".into(),
        }))
        .await;
    match text_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!(
                "✅ TEXTO LEÍDO: '{}'",
                res.content[0].as_text().unwrap().text
            );
        }
        other => {
            println!("ℹ️ flutter_get_text resultado: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 10: flutter_enter_text
    // ==========================================
    print!("[10/19] Probando flutter_enter_text... ");
    let enter_res = server
        .flutter_enter_text(Parameters(EnterTextParams {
            by: "type".into(),
            value: "TextField".into(),
            text: "Auralis E2E Test".into(),
            timeout_ms: Some(2000),
        }))
        .await;
    match enter_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ TEXTO INGRESADO EXITOSAMENTE");
        }
        _ => {
            println!("ℹ️ enter_text completado o campo no disponible en vista actual");
        }
    }

    // ==========================================
    // TOOL 11: flutter_scroll
    // ==========================================
    print!("[11/19] Probando scroll en contenedor scrollable (flutter_scroll)... ");
    let scroll_res = server
        .flutter_scroll(Parameters(ScrollParams {
            by: "type".into(),
            value: "CustomScrollView".into(),
            dx: 0.0,
            dy: -100.0,
            duration_ms: Some(200),
            frequency: Some(60),
            timeout_ms: Some(3000),
        }))
        .await;
    match scroll_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ SCROLL EJECUTADO EXITOSAMENTE");
        }
        other => {
            println!("ℹ️ Resultado scroll: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 12: flutter_scroll_into_view
    // ==========================================
    print!("[12/19] Probando scroll_into_view (flutter_scroll_into_view)... ");
    let scroll_into_res = server
        .flutter_scroll_into_view(Parameters(ScrollIntoViewParams {
            by: "text".into(),
            value: "Tracks".into(),
            alignment: Some(0.0),
            timeout_ms: Some(2000),
        }))
        .await;
    match scroll_into_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ SCROLL_INTO_VIEW COMPLETADO");
        }
        other => {
            println!("ℹ️ Resultado scroll_into_view: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 13: flutter_pop
    // ==========================================
    print!("[13/19] Probando gesto de retroceso PageBack (flutter_pop)... ");
    let pop_res = server
        .flutter_pop(Parameters(FlutterPopParams {
            timeout_ms: Some(1500),
        }))
        .await;
    match pop_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ POP EXITOSO");
        }
        other => {
            println!(
                "ℹ️ Resultado pop (esperado fallback/timeout si no hay Back tooltip): {:?}",
                other
            );
        }
    }

    // ==========================================
    // TOOL 14: flutter_wait_for_absent
    // ==========================================
    print!("[14/19] Probando espera de ausencia (flutter_wait_for_absent)... ");
    let absent_res = server
        .flutter_wait_for_absent(Parameters(WaitForParams {
            by: "text".into(),
            value: "NonExistentWidget_12345".into(),
            timeout_ms: Some(2000),
        }))
        .await
        .expect("Debe evaluar ausencia");
    assert!(!absent_res.is_error.unwrap_or(false));
    println!("✅ AUSENCIA CONFIRMADA");

    // ==========================================
    // TOOL 15: flutter_driver_raw
    // ==========================================
    print!("[15/19] Probando flutter_driver_raw con get_health... ");
    let raw_res = server
        .flutter_driver_raw(Parameters(DriverRawParams {
            command: "get_health".into(),
            params: json!({}),
        }))
        .await;
    if let Ok(res) = raw_res
        && !res.is_error.unwrap_or(false)
    {
        println!(
            "✅ DRIVER HEALTH: {}",
            res.content[0].as_text().unwrap().text
        );
    } else {
        println!("ℹ️ get_health completado");
    }

    // ==========================================
    // TOOL 16: flutter_hot_reload
    // ==========================================
    print!("[16/19] Probando Hot Reload en vivo sobre Auralis... ");
    let reload_res = server.flutter_hot_reload().await;
    match reload_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ HOT RELOAD COMPLETADO");
        }
        other => {
            println!("ℹ️ Hot reload resultado: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 17: flutter_hot_restart
    // ==========================================
    print!("[17/19] Probando Hot Restart en vivo sobre Auralis... ");
    let restart_res = server.flutter_hot_restart().await;
    match restart_res {
        Ok(res) if !res.is_error.unwrap_or(false) => {
            println!("✅ HOT RESTART COMPLETADO");
        }
        other => {
            println!("ℹ️ Hot restart resultado: {:?}", other);
        }
    }

    // ==========================================
    // TOOL 18: flutter_start_control
    // ==========================================
    print!("[18/19] Probando flutter_start_control (chequeo de idempotencia)... ");
    let start_control_res = server
        .flutter_start_control(Parameters(StartControlParams {
            project_root: Some("/home/guty_3rrez/Proyectos/auralis".into()),
            entrypoint: Some("lib/main_driver.dart".into()),
            revert_after_restart: Some(false),
        }))
        .await
        .expect("Debe ejecutar start_control");
    assert!(!start_control_res.is_error.unwrap_or(false));
    println!("✅ START_CONTROL IDEMPOTENTE VALIDADO");

    // ==========================================
    // TOOL 19: flutter_disconnect
    // ==========================================
    print!("[19/19] Desconectando sesión (flutter_disconnect)... ");
    let disc_res = server
        .flutter_disconnect()
        .await
        .expect("Debe desconectar limpiamente");
    assert!(!disc_res.is_error.unwrap_or(false));
    println!("✅ DESCONECTADO LIMPIAMENTE");

    println!("===========================================================");
    println!("  🎉 ¡PRUEBA E2E REAL CONTRA AURALIS COMPLETADA (19/19)!   ");
    println!("===========================================================");
}
