# Estado del Proyecto y Tareas Pendientes: `flutter-native-mcp`

Este documento consolida el estado actual del servidor MCP nativo para Flutter en Rust, las metas alcanzadas y las tareas pendientes/roadmap para futuras sesiones.

---

## 1. Estado Actual y Logros Completados ✅

### Arquitectura Hexagonal
- **Dominio (`src/domain/`):**
  - Entidades: `WidgetNode` (árbol semántico podado con `semantics_label`), `Finder` (búsqueda por Key, Text, Tooltip, Type, SemanticsLabel y serialización directa para Driver), `Gesture` (Tap, EnterText, ClearText, Scroll, ScrollIntoView, ScrollUntilVisible).
  - Servicios de Dominio: `TreePruner` enriquecido con extracción de textos en `Text`, títulos, tooltips y etiquetas semánticas, filtrando ruido estructural (`Padding`, `SizedBox`, `DecoratedBox`, `Transform`, etc.).
- **Aplicación (`src/application/`):**
  - Puerto Inbound (`FlutterAppService`): Contrato completo de casos de uso (13 operaciones).
  - Puerto Outbound (`FlutterVmPort`): SPI desacoplado del protocolo subyacente (`get_text`, `wait_for`, `wait_for_absent`, `trigger_hot_restart`, etc.).
  - Caso de uso (`FlutterServiceImpl`): Implementación desacoplada y probada con mocks unitarios.
- **Infraestructura (`src/infrastructure/`):**
  - Outbound: `WebSocketVmServiceAdapter` con cliente JSON-RPC 2.0 sobre WebSockets (`tokio-tungstenite`) comunicándose con `ext.flutter.inspector` y `ext.flutter.driver`.
  - Outbound Mock: `MockVmServiceAdapter` con soporte para todas las operaciones simuladas.
  - Inbound: `FlutterMcpServer` implementado con la librería oficial `rmcp` (v3.4.0) comunicándose por `stdio`.

### Catálogo Completo de Herramientas MCP Expuestas (13 Herramientas)
1. `flutter_connect`: Conexión al WebSocket del Dart VM Service de la app Flutter activa.
2. `flutter_disconnect`: Cierre limpio de la sesión activa del Dart VM Service.
3. `flutter_snapshot`: Obtención del árbol podado de UI en formato JSON semántico.
4. `flutter_tap`: Toque nativo sobre widgets (por clave, texto, tooltip, tipo o semantics).
5. `flutter_enter_text`: Escritura de texto en campos interactivos / `TextField`.
6. `flutter_get_text`: Extracción del contenido textual visible de cualquier widget.
7. `flutter_scroll`: Desplazamiento programático en contenedores (`ListView`, `CustomScrollView`) con `dx`, `dy`, `duration_ms` y `frequency`.
8. `flutter_scroll_into_view`: Desplazamiento automático hasta hacer visible un widget en pantalla con alineación configurable.
9. `flutter_wait_for`: Espera asíncrona a que aparezca un widget con timeout configurable.
10. `flutter_wait_for_absent`: Espera asíncrona a que desaparezca un widget (loaders, modales).
11. `flutter_hot_reload`: Recarga en caliente instantánea sin perder estado ni reiniciar proceso.
12. `flutter_hot_restart`: Reinicio completo / Reassemble de la app vía Dart VM Service.
13. `flutter_screenshot`: Captura nativa de pantalla (PNG) con soporte para guardado automático en disco (`save_path`).

### Arnés de Pruebas y Calidad (100% Verde)
- **Unit Tests:** Cobertura exhaustiva en dominio y aplicación (`cargo test --lib`).
- **Integration Tests:** Pruebas integrales de flujo hexagonal con mocks en `tests/integration_test.rs`.
- **BDD (Cucumber / Gherkin):** Escenarios de comportamiento ejecutados mediante `cucumber-rs`.
- **Auditoría de Seguridad:** `cargo audit` con 0 vulnerabilidades reportadas.
- **Linter & Formato:** `cargo clippy --all-targets -- -D warnings` (0 advertencias) y `cargo fmt`.
- **Script de Verificación:** [`scripts/verify_harness.sh`](file:///home/guty_3rrez/Proyectos/flutter-native-mcp/scripts/verify_harness.sh) ejecuta todos los gates de calidad.

### Integración en Antigravity
- Binario de producción compilado en release e instalado en `/home/guty_3rrez/.local/bin/flutter-native-mcp`.
- Esquemas JSON actualizados en `~/.gemini/antigravity-cli/mcp/flutter_native/` para las 13 herramientas.
- Configuración en `~/.gemini/config/mcp_config.json` bajo el nombre `flutter_native`.
- Validación en vivo contra Auralis en Linux Desktop.

---

## 2. Roadmap y Próximas Mejoras Opcionales 📋

### 1. Inyección Directa Sin Extensión Driver (Zero-Touch)
- [ ] Implementar inyección de eventos sintéticos (`PointerDownEvent` / `PointerUpEvent`) mediante `evaluate()` llamando directamente a `GestureBinding.instance.handleEvent` para apps que no tengan `enableFlutterDriverExtension()` activado.

### 2. Auto-detección de URI de Dart VM
- [ ] Implementar herramienta o flag que escanee puertos locales o el archivo de log de Flutter para conectarse automáticamente sin requerir que el usuario pegue la URL del WebSocket.

### 3. CI/CD Automatizado
- [ ] Configurar `.github/workflows/ci.yml` con ejecución automática de `verify_harness.sh` y publicación de binarios precompilados para Linux, macOS y Windows.
