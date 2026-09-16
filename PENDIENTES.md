# Tareas Pendientes y Próximos Pasos (Roadmap v1.0)
## Proyecto: `flutter-native-mcp`

> **Estado Actual (v0.1.0 MVP):** Funcional y probado en vivo contra Auralis en Linux Desktop.  
> Binario instalado en: `~/.local/bin/flutter-native-mcp`  
> Configuración global: `~/.gemini/config/mcp_config.json`  

---

## 🎯 Resumen de Lo Completado (Iteración 1)
- [x] **Arquitectura Hexagonal**: Capas desacopladas (Dominio, Aplicación, Infraestructura).
- [x] **Servidor MCP en Rust**: Implementado con SDK oficial `rmcp 3.4.0` sobre `stdio`.
- [x] **Arnés de Calidad y Pruebas**:
  - Pruebas Unitarias (`cargo test --lib`).
  - Pruebas de Integración (`tests/integration_test.rs`).
  - Pruebas de Comportamiento BDD con Gherkin (`cucumber-rs` en `tests/bdd.rs`).
  - Linter estricto: `cargo clippy --all-targets -- -D warnings` (0 advertencias).
  - Auditoría de Seguridad: `cargo audit` contra RustSec (0 vulnerabilidades).
  - Script unificado de verificación: `./scripts/verify_harness.sh`.
- [x] **Integración con Dart VM Service**: Conexión WebSocket (`tokio-tungstenite`) y autodescubrimiento de Isolate.
- [x] **UI Snapshot & Poda Semántica (`TreePruner`)**: Poda de contenedores de layout (`Padding`, `SizedBox`, etc.) reduciendo el árbol a <2ms y bajo costo de tokens.
- [x] **Acciones y Gestos Nativos**:
  - `flutter_tap` (por Key, texto, tooltip o tipo).
  - `flutter_enter_text` (simulación de teclado nativo en campos de texto).
  - `flutter_hot_reload` (recarga en caliente instantánea sin reiniciar proceso).
  - `flutter_screenshot` (decodificación de imagen PNG rasterizada del motor Flutter).
- [x] **Validación en Vivo**: Probado contra Auralis en Linux Desktop (ingreso de texto en buscador, apertura de modal Settings y capturas visuales).

---

## ⏳ Tareas Pendientes para Próximas Sesiones (Iteración 2)

### 1. Gestos Avanzados de Scroll
- [ ] **Completar `ScrollUntilVisible`**:
  - En `src/infrastructure/outbound/vm_service_client.rs`, la variante `Gesture::ScrollUntilVisible` está como stub.
  - Implementar el bucle iterativo que desplace la vista (`scroll`) paso a paso hasta que el elemento objetivo esté montado en el viewport y sea visible.
- [ ] **Soporte para Scroll por Teclado / Rueda**:
  - Añadir soporte para delta horizontal y vertical configurable por el agente.

### 2. Herramientas MCP de Sincronización y Espera
- [ ] **Exponer `flutter_wait_for`**:
  - Permite al agente esperar de forma asíncrona a que un widget aparezca en pantalla con un timeout configurable (ej. tras una petición de red o navegación).
- [ ] **Exponer `flutter_wait_for_absent`**:
  - Permite verificar y esperar que un modal, diálogo o indicador de carga (`CircularProgressIndicator`) desaparezca antes de continuar.

### 3. Diagnóstico y Captura de Errores de Renderizado
- [ ] **Exponer `flutter_get_errors`**:
  - Consultar los errores registrados en Flutter (como los `RenderFlex overflow` detectados en Auralis).
  - Permitir que los agentes de IA no solo interactúen, sino que puedan auditar y corregir bugs visuales automáticamente.

### 4. Lanzador de Aplicaciones Integrado (`flutter_launch_app`)
- [ ] **Arranque en Segundo Plano**:
  - Crear una herramienta MCP que reciba la ruta del proyecto Flutter y el target (`-d linux`, `-d chrome`, etc.).
  - Lanzar el proceso `flutter run` en segundo plano, extraer automáticamente la URL del Dart VM Service de los logs de inicio y establecer la conexión sin intervención manual.

### 5. Modo Gestual Zero-Touch (Sin `flutter_driver`)
- [ ] **Inyección directa por Dart VM `evaluate`**:
  - Evaluar la inyección de `PointerDownEvent` / `PointerUpEvent` directamente a través de `GestureBinding.instance.handleEvent` invocada mediante `evaluate()` en la Dart VM, permitiendo taps sin requerir `enableFlutterDriverExtension()` en la app.

### 6. Pipeline de CI/CD (GitHub Actions)
- [ ] Crear `.github/workflows/ci.yml` que ejecute en cada pull request:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --all-targets`
  - `cargo audit`
  - `cargo mutants`

---

## 🛠️ Cómo Retomar en la Próxima Sesión

1. **Navegar al repositorio:**
   ```bash
   cd /home/guty_3rrez/Proyectos/flutter-native-mcp
   ```
2. **Ejecutar el arnés de pruebas para verificar que todo sigue en verde:**
   ```bash
   ./scripts/verify_harness.sh
   ```
3. **Probar el binario MCP directamente:**
   ```bash
   flutter-native-mcp
   ```
4. **Recompilar e instalar cambios tras editar código:**
   ```bash
   cargo build --release && cp target/release/flutter-native-mcp ~/.local/bin/
   ```
