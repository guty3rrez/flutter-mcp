# Estado del Proyecto y Tareas Pendientes: `flutter-native-mcp`

Este documento consolida el estado actual del servidor MCP nativo para Flutter en Rust, las metas alcanzadas y las tareas pendientes/roadmap para futuras sesiones.

---

## 1. Estado Actual y Logros Completados ✅

### Arquitectura Hexagonal
- **Dominio (`src/domain/`):**
  - Entidades: `WidgetNode` (árbol semántico podado), `Finder` (búsqueda por Key, Text, Tooltip, Type), `Gesture` (Tap, EnterText, Scroll).
  - Servicios de Dominio: `TreePruner` para eliminar ruido de maquetación (`Padding`, `SizedBox`, `DecoratedBox`, `Transform`, etc.) y retener elementos interactivos y con significado para el LLM.
- **Aplicación (`src/application/`):**
  - Puerto Inbound (`FlutterAppService`): Contrato de casos de uso para orquestar la app.
  - Puerto Outbound (`FlutterVmPort`): SPI desacoplado del protocolo subyacente.
  - Caso de uso (`FlutterServiceImpl`): Implementación agnóstica de infraestructura.
- **Infraestructura (`src/infrastructure/`):**
  - Outbound: `WebSocketVmServiceAdapter` con cliente JSON-RPC 2.0 sobre WebSockets (`tokio-tungstenite`) para comunicarse con el Dart VM Service de Flutter (`ext.flutter.inspector` y `ext.flutter.driver`).
  - Outbound Mock: `MockVmServiceAdapter` para pruebas unitarias sin dependencias externas.
  - Inbound: `FlutterMcpServer` implementado con la librería oficial `rmcp` (v3.4.0) comunicándose por `stdio`.

### Herramientas MCP Expuestas
1. `flutter_connect`: Conexión al WebSocket de la app Flutter activa.
2. `flutter_snapshot`: Obtención del árbol podado de UI en formato JSON semántico.
3. `flutter_tap`: Toque nativo sobre widgets (por clave, texto, tooltip o tipo).
4. `flutter_enter_text`: Escritura de texto en campos de formulario / `TextField`.
5. `flutter_screenshot`: Captura nativa del framebuffer en formato PNG.
6. `flutter_hot_reload`: Disparo de recarga en caliente vía Dart VM.

### Arnés de Pruebas y Calidad
- **Unit Tests:** Cobertura de podado de árbol, deserialización de diagnósticos y lógica de búsqueda.
- **Integration Tests:** Pruebas completas de puertos y adaptadores con mocks deterministas.
- **BDD (Cucumber / Gherkin):** Escenarios escritos en `.feature` y ejecutados mediante `cucumber-rs`.
- **Auditoría de Seguridad:** `cargo audit` con 0 vulnerabilidades.
- **Linter & Formato:** `cargo clippy -- -D warnings` y `cargo fmt` pasando al 100%.
- **Script de Verificación:** [`scripts/verify_harness.sh`](file:///home/guty_3rrez/Proyectos/flutter-native-mcp/scripts/verify_harness.sh) ejecuta toda la suite en un solo paso.

### Integración en Antigravity
- Binario de producción instalado en `/home/guty_3rrez/.local/bin/flutter-native-mcp`.
- Configurado en `~/.gemini/config/mcp_config.json` bajo el nombre `flutter_native`.
- Validación en vivo contra la aplicación de escritorio **Auralis** ejecutando acciones de navegación, entrada de texto, screenshots y hot reload en tiempo real.

---

## 2. Tareas Pendientes y Roadmap Futuro 📋

### Fase 1: Extensión de Gestos e Interacciones
- [ ] **Soporte para Scroll:** Implementar herramienta MCP `flutter_scroll(by, value, dx, dy, duration_ms)` para listas (`ListView`, `CustomScrollView`).
- [ ] **Soporte para Drag & Drop:** Implementar `flutter_drag(from_finder, to_offset)`.
- [ ] **Long Press & Doble Tap:** Añadir variantes de gestos mediante comandos de `ext.flutter.driver`.
- [ ] **Envío de Teclas Especiales:** Soporte para teclas del teclado (`Enter`, `Backspace`, `Tab`, `Escape`) en `flutter_enter_text` o herramienta dedicada `flutter_key_event`.

### Fase 2: Enriquecimiento del Inspector de UI
- [ ] **Extracción Profunda de Textos:** Extraer contenido textual de widgets complejos como `RichText` y `TextSpan` dentro de `TreePruner` cuando no provengan directamente de la propiedad `data`.
- [ ] **Filtro de Coordenadas de Bounding Box:** Incorporar cálculo de rectángulos (`bounds`: left, top, width, height) a partir de las propiedades de RenderObject (`RenderBox.size` y `localToGlobal`).
- [ ] **Búsqueda Compuesta:** Permitir combinaciones lógicas en `Finder` (ej. buscar un `IconButton` que contenga un `Icon` específico o esté dentro de un padre dado).

### Fase 3: Capacidades Avanzadas de MCP
- [ ] **Retorno Multimedia (`ContentBlock::image`):** Enviar el screenshot directamente como bloque de imagen en base64 en la respuesta de `flutter_screenshot` para que los clientes MCP compatibles con imágenes lo visualicen en línea.
- [ ] **Reconexión Automática:** Detección de caída de conexión y reconexión automática si el proceso Flutter se reinicia (`hot restart`).
- [ ] **Detección Automática de URI del Dart VM:** Agregar un subcomando o script que localice automáticamente la URI de depuración activa leyendo los logs o mediante `lsof`/procesos locales.

### Fase 4: Pruebas de Mutación y CI/CD
- [ ] **Pipeline con `cargo-mutants`:** Configurar un pipeline o job programado de pruebas de mutación para asegurar la resiliencia de los tests ante cambios en la lógica de negocio.
- [ ] **Workflow de GitHub Actions:** Crear workflow para build multiplataforma (Linux, macOS, Windows) y publicación automática de releases binarios.
