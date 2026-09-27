---
name: flutter-mcp
description: "Controla, inspecciona y prueba aplicaciones Flutter en tiempo real mediante Flutter Driver y Dart VM Service sin modificar el código de producción (patrón lib/main_driver.dart). Usar cuando el usuario pida probar la app en vivo, automatizar flujos UI, verificar widgets, tomar screenshots, o depurar logs y rendimiento de Flutter."
---

# Flutter Native MCP Guide & Driver Protocol

Esta skill enseña cómo interactuar con aplicaciones Flutter en ejecución utilizando el servidor MCP `flutter-native-mcp` (`mcp__flutter_mcp__*` o `flutter_mcp`). Permite inspeccionar el árbol de widgets, interactuar con la interfaz (clicks, texto, scroll), diagnosticar logs, capturar pantallas de GPU y medir rendimiento en tiempo real sin requerir Appium, Selenium ni frameworks externos pesados.

---

## Cuando cargar esta skill

Cargar cuando el usuario o la tarea involucre:
- Probar o automatizar una app Flutter en tiempo real contra un emulador, dispositivo físico o app de escritorio (Linux/macOS/Windows).
- Tomar capturas de pantalla de la app en ejecución (`flutter_screenshot`).
- Inspeccionar el árbol visual de widgets activos (`flutter_snapshot`).
- Automatizar interacciones de usuario (`flutter_tap`, `flutter_enter_text`, `flutter_scroll`, `flutter_wait_for`).
- Depurar excepciones en tiempo real, pantallas rojas o logs de Flutter (`flutter_get_errors`, `flutter_get_logs`).
- Medir jank y rendimiento de cuadros (`flutter_get_performance`).
- Validar una feature en vivo en desarrollo local antes de darla por terminada.

No cargar para: pruebas puramente unitarias que no interactúen con una app Flutter corriendo, o tareas de backend ajenas a Flutter.

---

## ⚠️ La Regla de Oro: Patrón No Destructivo `lib/main_driver.dart`

> [!IMPORTANT]
> **PROHIBIDO MODIFICAR `lib/main.dart` PARA HABILITAR FLUTTER DRIVER.**
> Modificar el archivo principal de producción introduce dependencias de test en el código final, ensucia el árbol de Git y genera riesgos de filtración a builds de release.

En su lugar, se debe utilizar siempre un **entrypoint secundario**:

```dart
// lib/main_driver.dart
import 'package:flutter_driver/driver_extension.dart';
import 'main.dart' as app;

void main() {
  enableFlutterDriverExtension();
  app.main();
}
```

---

## Workflow Completo

### Fase 1: Preparación del Entrypoint en la App Objetivo

1. **Verificar `pubspec.yaml`**:
   Asegurar que `flutter_driver` esté en `dev_dependencies`:
   ```bash
   flutter pub add --dev flutter_driver --sdk=flutter
   ```
2. **Crear o verificar `lib/main_driver.dart`**:
   Si no existe, crear el archivo con el snippet indicado arriba.
3. **Ejecutar la app apuntando al entrypoint del driver**:
   - En Linux Desktop:
     ```bash
     flutter run -d linux -t lib/main_driver.dart
     ```
   - O usando `monty` si el proyecto lo gestiona:
     ```bash
     monty run --target lib/main_driver.dart
     ```
4. **Obtener la URI del Dart VM Service**:
   Al iniciar, Flutter imprimirá en la consola una línea como:
   ```text
   The Dart VM Service is listening on http://127.0.0.1:41235/AbCdEfGh12=/
   ```
   O el WebSocket directo:
   ```text
   ws://127.0.0.1:41235/AbCdEfGh12=/ws
   ```

---

### Fase 2: Conexión con `flutter-native-mcp`

Conectar el servidor MCP al WebSocket de la app:

```json
// Tool: flutter_connect
{
  "vm_service_uri": "ws://127.0.0.1:41235/AbCdEfGh12=/ws"
}
```
*(Nota: el servidor acepta tanto la URL `http://` como `ws://`, convirtiéndola automáticamente al endpoint WebSocket).*

---

### Fase 3: Inspección Visual y Localización de Widgets

Antes de intentar hacer tap o escribir, mapea la pantalla para obtener los selectores precisos:

1. **Obtener el snapshot podado**:
   ```json
   // Tool: flutter_snapshot
   {}
   ```
   Devuelve un árbol JSON compacto y enriquecido (`WidgetNode`) que descarta contenedores vacíos y resalta nodos interactivos.

2. **Estrategia de selección de widgets (`Finder`)**:
   - **Prioridad 1 (`by: "key"`)**: Siempre preferido si el widget tiene `ValueKey` o `Key`. Inmune a traducciones y cambios de texto.
     ```json
     { "finder": { "by": "key", "value": "btn_login" } }
     ```
   - **Prioridad 2 (`by: "text"`)**: Busca texto visible.
     ```json
     { "finder": { "by": "text", "value": "Iniciar Sesión" } }
     ```
   - **Prioridad 3 (`by: "tooltip"` o `by: "semantics"`)**: Útil para IconButton o elementos accesibles sin texto explícito.
     ```json
     { "finder": { "by": "tooltip", "value": "Buscar canciones" } }
     ```
   - **Prioridad 4 (`by: "type"`)**: Para elementos estructurales (ej. `"ElevatedButton"`, `"Scrollable"`).

---

### Fase 4: Interacción y Gestos

1. **Tap / Clic**:
   ```json
   // Tool: flutter_tap
   {
     "finder": { "by": "key", "value": "btn_submit" }
   }
   ```
2. **Ingreso de Texto**:
   *Nota: `flutter_enter_text` realiza automáticamente un tap de enfoque previo para activar el cursor/IME del sistema antes de escribir.*
   ```json
   // Tool: flutter_enter_text
   {
     "finder": { "by": "key", "value": "input_email" },
     "text": "test@example.com"
   }
   ```
3. **Scroll**:
   - Scroll relativo:
     ```json
     // Tool: flutter_scroll
     {
       "finder": { "by": "type", "value": "Scrollable" },
       "dx": 0.0,
       "dy": -300.0
     }
     ```
   - Scroll hasta visibilidad:
     ```json
     // Tool: flutter_scroll_into_view
     {
       "finder": { "by": "text", "value": "Elemento al final de la lista" }
     }
     ```
4. **Esperas Asíncronas**:
   Nunca asumas un tiempo fijo (`sleep`); usa esperas basadas en condición:
   ```json
   // Tool: flutter_wait_for
   {
     "finder": { "by": "key", "value": "home_dashboard" },
     "timeout_ms": 10000
   }
   ```
   O para esperar que un loader desaparezca:
   ```json
   // Tool: flutter_wait_for_absent
   {
     "finder": { "by": "type", "value": "CircularProgressIndicator" },
     "timeout_ms": 5000
   }
   ```

---

### Fase 5: Evidencia Visual, Logs y Rendimiento

1. **Captura de Pantalla GPU**:
   ```json
   // Tool: flutter_screenshot
   {}
   ```
   Devuelve los bytes en Base64 directamente renderizados por el rasterizador de Flutter (independiente de gestores de ventanas del SO).

2. **Inspección de Excepciones y Pantallas Rojas**:
   ```json
   // Tool: flutter_get_errors
   {
     "clear": true
   }
   ```
   Detecta automáticamente bloques de error de Flutter (`EXCEPTION CAUGHT BY WIDGETS LIBRARY`, unhandled exceptions, etc.).

3. **Lectura de Logs (stdout/stderr/logging)**:
   ```json
   // Tool: flutter_get_logs
   {
     "limit": 50
   }
   ```

4. **Análisis de Rendimiento y Jank de Cuadros**:
   ```json
   // Tool: flutter_get_performance
   {}
   ```
   Analiza los eventos del VM Timeline (`getVMTimeline`) calculando tiempos de Build/Raster y detectando si hubo cuadros que superaron los 16.6ms (60 FPS) o 8.3ms (120 FPS).

---

### Fase 6: Ciclo de Vida y Hot Reload / Restart

- `flutter_hot_reload`: Aplica cambios instantáneos de código Dart sin perder el estado de la app.
- `flutter_hot_restart`: Reinicia la app completa y re-inicializa el estado de Flutter Driver automáticamente.
- `flutter_disconnect`: Cierra limpiamente la sesión WebSocket y drena los buffers.

---

## Manejo de Errores Típicos

- **Error `-32601 Method not found` en `ext.flutter.driver`**:
  Ocurre cuando la app se arrancó con `lib/main.dart` en vez de `lib/main_driver.dart`.
  *Solución*: Crear `lib/main_driver.dart` y reiniciar con `flutter run -t lib/main_driver.dart`.
- **`TimeoutException` en finders**:
  El widget no existe en pantalla. Ejecuta `flutter_snapshot` para verificar qué widgets están montados y prefiere `by: "key"`.
- **Widgets perezosos en `ListView` / `SliverList`**:
  Si un elemento está fuera de la pantalla, usa `flutter_scroll_into_view` para forzar su montaje en el árbol antes de interactuar.

---

## Reglas de Scope — DO NOT

- **NO** modificar `lib/main.dart` para activar Flutter Driver; usar siempre `lib/main_driver.dart`.
- **NO** usar `sleep` para esperar animaciones o respuestas de red; usar `flutter_wait_for` o `flutter_wait_for_absent`.
- **NO** inventar keys o finders sin consultar previamente el árbol con `flutter_snapshot`.
- **NO** dejar procesos huérfanos de Flutter en segundo plano al finalizar las pruebas.
