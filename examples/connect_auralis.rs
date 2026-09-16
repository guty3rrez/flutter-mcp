use std::sync::Arc;
use flutter_native_mcp::{
    FlutterAppService,
    FlutterServiceImpl,
    FlutterVmPort,
    WebSocketVmServiceAdapter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vm_uri = std::env::args().nth(1).unwrap_or_else(|| {
        "ws://127.0.0.1:33001/4LSO1e6XxM0=/ws".to_string()
    });

    println!("===========================================================");
    println!("  Conectando a Auralis Music Player vía WebSocket Dart VM  ");
    println!("  Target: {vm_uri}");
    println!("===========================================================");

    let adapter = Arc::new(WebSocketVmServiceAdapter::new());
    let service = FlutterServiceImpl::new(adapter.clone());

    // 1. Conectar
    print!("1. Conectando al Isolate de Flutter... ");
    service.connect(&vm_uri).await?;
    println!("✅ ¡CONECTADO!");

    // 2. Extraer árbol y podar
    println!("\n2. Obteniendo árbol de widgets activo (Snapshot)...");
    let raw_tree = adapter.get_diagnostics_tree(50).await?;
    println!("DEBUG RAW JSON KEYS: {:?}", raw_tree.as_object().map(|o| o.keys().collect::<Vec<_>>()));
    if let Some(desc) = raw_tree.get("description") {
        println!("Root description: {:?}", desc);
    }
    if let Some(result) = raw_tree.get("result") {
        println!("Result keys: {:?}", result.as_object().map(|o| o.keys().collect::<Vec<_>>()));
    }

    let snapshot = service.get_pruned_snapshot().await?;
    
    println!("✅ Árbol podado obtenido con éxito.");
    println!("\n🔍 Buscando elementos interactivos y con Key en Auralis:");

    fn find_interesting_widgets(node: &flutter_native_mcp::WidgetNode, list: &mut Vec<String>) {
        if node.key.is_some() || node.text.is_some() || node.is_interactive {
            list.push(format!(
                "- [{}] key: {:?}, text: {:?}, tooltip: {:?}, interactive: {}",
                node.widget_type, node.key, node.text, node.tooltip, node.is_interactive
            ));
        }
        for child in &node.children {
            find_interesting_widgets(child, list);
        }
    }

    let mut interesting = Vec::new();
    find_interesting_widgets(&snapshot, &mut interesting);
    for item in interesting.iter().take(30) {
        println!("{item}");
    }

    // 3. Probar Interacción Nativa (Tap y EnterText en el buscador)
    println!("\n3. Probando interacción nativa sobre TextField de Auralis...");
    match service.tap(flutter_native_mcp::Finder::by_type("TextField")).await {
        Ok(_) => println!("✅ ¡TAP EN TEXTFIELD EXITOSO!"),
        Err(e) => println!("⚠️ Error en tap: {e}"),
    }

    match service.enter_text(flutter_native_mcp::Finder::by_type("TextField"), "Daft Punk - Discovery".to_string()).await {
        Ok(_) => println!("✅ ¡TEXTO 'Daft Punk - Discovery' INGRESADO EN TEXTFIELD DE AURALIS!"),
        Err(e) => println!("⚠️ Error ingresando texto: {e}"),
    }

    // 4. Tomar captura de pantalla nativa
    println!("\n4. Capturando pantalla de la interfaz...");
    match service.take_screenshot().await {
        Ok(bytes) => {
            let path = "/home/guty_3rrez/Proyectos/flutter-native-mcp/auralis_native_screenshot.png";
            tokio::fs::write(path, &bytes).await?;
            println!("✅ ¡SCREENSHOT NATIVO GUARDADO EN: {path} ({} bytes)!", bytes.len());
        }
        Err(e) => println!("⚠️ Error capturando screenshot: {e}"),
    }

    // 5. Probar navegación haciendo tap en "Settings"
    println!("\n5. Navegando: haciendo tap nativo en 'Settings'...");
    match service.tap(flutter_native_mcp::Finder::by_text("Settings", true)).await {
        Ok(_) => {
            println!("✅ ¡TAP EN 'Settings' EXITOSO!");
            tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

            let path_settings = "/home/guty_3rrez/Proyectos/flutter-native-mcp/auralis_settings_screenshot.png";
            match service.take_screenshot().await {
                Ok(bytes) => {
                    tokio::fs::write(path_settings, &bytes).await?;
                    println!("✅ ¡CAPTURA DE PANTALLA TRAS TAP EN SETTINGS GUARDADA EN: {path_settings}!");
                }
                Err(e) => println!("⚠️ Error capturando screenshot de settings: {e}"),
            }
        }
        Err(e) => println!("⚠️ Error navegando a Settings: {e}"),
    }

    // 6. Probar Hot Reload
    print!("\n6. Probando Hot Reload en vivo vía Dart VM Service... ");
    service.hot_reload().await?;
    println!("✅ ¡HOT RELOAD COMPLETADO EN LA APP!");

    println!("\n===========================================================");
    println!("  🎉 ¡PRUEBA DE INTEGRACIÓN EN VIVO EXITOSA!               ");
    println!("===========================================================");

    Ok(())
}
