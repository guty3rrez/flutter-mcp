use flutter_mcp::{Finder, FlutterAppService, FlutterServiceImpl, MockVmServiceAdapter};
use std::sync::Arc;

#[tokio::test]
async fn test_full_hexagonal_flow_with_mock_vm() {
    // 1. Instanciar adaptador secundario simulado
    let mock_adapter = Arc::new(MockVmServiceAdapter::new());

    // 2. Instanciar capa de aplicación (puerto primario) inyectando el puerto secundario
    let app_service = FlutterServiceImpl::new(mock_adapter.clone());

    // 3. Conectar a la app simulada
    app_service
        .connect("ws://127.0.0.1:45678/ws")
        .await
        .expect("Debe conectar exitosamente");

    // 4. Obtener snapshot podado
    let snapshot = app_service
        .get_pruned_snapshot()
        .await
        .expect("Debe obtener y podar el snapshot");

    assert_eq!(snapshot.widget_type, "Scaffold");
    assert_eq!(snapshot.children.len(), 2);
    // El AppBar con su texto
    assert_eq!(snapshot.children[0].widget_type, "AppBar");
    // El Padding ruidoso se podó dejando directamente el ElevatedButton
    assert_eq!(snapshot.children[1].widget_type, "ElevatedButton");
    assert_eq!(
        snapshot.children[1].key.as_deref(),
        Some("[<'btn_counter'>]")
    );

    // 5. Ejecutar un tap sobre el botón
    app_service
        .tap(Finder::by_key("btn_counter"))
        .await
        .expect("Debe despachar el tap");

    // 6. Obtener texto del widget
    let text = app_service
        .get_text(Finder::by_key("btn_counter"))
        .await
        .expect("Debe obtener texto");
    assert_eq!(text, "Text for btn_counter");

    // 7. Ejecutar scroll y scroll_into_view
    app_service
        .scroll(Finder::by_type("ListView"), 0.0, -150.0, 300, 60)
        .await
        .expect("Debe despachar scroll");

    app_service
        .scroll_into_view(Finder::by_key("footer_item"), 0.5)
        .await
        .expect("Debe despachar scroll_into_view");

    // 8. Probar esperas sincronizadas
    app_service
        .wait_for(Finder::by_key("btn_counter"), 2000)
        .await
        .expect("Debe esperar widget");

    app_service
        .wait_for_absent(Finder::by_key("loading_spinner"), 2000)
        .await
        .expect("Debe esperar ausencia");

    // 9. Probar hot reload y hot restart
    app_service
        .hot_reload()
        .await
        .expect("Debe ejecutar hot reload");
    app_service
        .hot_restart()
        .await
        .expect("Debe ejecutar hot restart");

    // 10. Probar screenshot
    let screenshot = app_service
        .take_screenshot()
        .await
        .expect("Debe tomar screenshot");
    assert!(!screenshot.is_empty());

    // 11. Verificar que el adaptador secundario recibió todos los gestos
    let gestures = mock_adapter.get_dispatched_gestures().await;
    assert_eq!(gestures.len(), 3); // Tap, Scroll, ScrollIntoView

    // 12. Desconectar
    app_service
        .disconnect()
        .await
        .expect("Debe desconectar limpiamente");
}
