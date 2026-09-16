use flutter_native_mcp::{Finder, FlutterAppService, FlutterServiceImpl, MockVmServiceAdapter};
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

    // 6. Verificar que el adaptador secundario recibió el gesto
    let gestures = mock_adapter.get_dispatched_gestures().await;
    assert_eq!(gestures.len(), 1);

    // 7. Desconectar
    app_service
        .disconnect()
        .await
        .expect("Debe desconectar limpiamente");
}
