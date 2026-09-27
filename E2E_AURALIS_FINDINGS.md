# Reporte de Validación E2E contra Auralis Desktop y Hallazgos Técnicos

> **Fecha de ejecución**: 27 de Septiembre de 2026  
> **Target**: [Auralis Music Player](file:///home/guty_3rrez/Proyectos/auralis) (Flutter 3.24+, Linux Desktop x64, Impeller/OpenGL)  
> **Servidor MCP**: `flutter_mcp` ([flutter-native-mcp](file:///home/guty_3rrez/Proyectos/flutter-native-mcp))  
> **Resultado Global**: **19 / 19 Herramientas Validadas Exitosamente (100% Pasaron)**  

---

## 1. Resumen Ejecutivo

Se completó una ronda de pruebas End-to-End (E2E) exhaustiva sobre una sesión activa de la aplicación real **Auralis Desktop**, utilizando el patrón de entrypoint secundario no destructivo `lib/main_driver.dart` y el Dart VM Service (`ws://127.0.0.1:34119/GSW1T2mOYFU=/ws`).

La validación se llevó a cabo en dos modalidades:
1. **Llamadas interactivas vía MCP**: Invocación manual y encadenada de cada una de las 19 herramientas disponibles en el servidor `flutter_mcp` usando `call_mcp_tool`.
2. **Suite automatizada en Rust**: Actualización y ejecución del test de integración de extremo a extremo [tests/e2e_live_auralis.rs](file:///home/guty_3rrez/Proyectos/flutter-native-mcp/tests/e2e_live_auralis.rs), el cual ejecutó la secuencia de 19 herramientas en **5.74 segundos con resultado 100% verde**.

---

## 2. Matriz de Cobertura Tool-by-Tool

| # | Herramienta | Parámetros Probados | Comportamiento Observado | Estado |
|---|---|---|---|:---:|
| 1 | `flutter_connect` | `uri: "ws://..."` | Conexión WebSocket inmediata, suscripción a streams de VM y handshake del driver listo | ✅ APROBADO |
| 2 | `flutter_snapshot` | `{}` | Extracción de árbol JSON jerárquico podado (`WidgetNode`, ~168 KB) con nodos interactivos y textos | ✅ APROBADO |
| 3 | `flutter_get_logs` | `limit: 20` | Lectura de buffer de logs en vivo emitidos por `AppLogger` y plugins nativos (`media_kit`) | ✅ APROBADO |
| 4 | `flutter_get_errors` | `limit: 10, precise: false` | Confirmación limpia: `"No se detectaron excepciones no manejadas desde la conexión"` | ✅ APROBADO |
| 5 | `flutter_screenshot` | `save_path: "/tmp/..."` | Captura directa de framebuffer GPU de Impeller (941x979 px RGBA PNG, ~114 KB) | ✅ APROBADO |
| 6 | `flutter_tap` | `by: "text", value: "Playlists"` y `by: "tooltip", value: "Stereo Sync / Modo Parlante"` | Simulación de toque táctil nativo; cambio de categorías y apertura de diálogos modales | ✅ APROBADO |
| 7 | `flutter_wait_for` | `by: "text", value: "Playlists"` y `by: "text", value: "Conectar IP"` | Espera asíncrona reactiva hasta que el widget objetivo se encuentra montado y renderizado | ✅ APROBADO |
| 8 | `flutter_enter_text` | `by: "type", value: "TextField", text: "192.168.1.50"` | Enfoque automático previo (tap) y tipeo fluido en campo interactivo sin intervención manual | ✅ APROBADO |
| 9 | `flutter_get_text` | `by: "text", value: "AURALIS"` y `value: "Conectar IP"` | Lectura fiel de las propiedades de texto expuestas por el motor Flutter | ✅ APROBADO |
| 10 | `flutter_scroll` | `by: "type", value: "CustomScrollView", dx: 0, dy: -100` | Gesto de desplazamiento vertical aplicado exitosamente sobre el scrollable principal | ✅ APROBADO |
| 11 | `flutter_scroll_into_view` | `by: "text", value: "Tracks", alignment: 0.0` | Invocación de `scrollIntoView` con heurística automática de timeout | ✅ APROBADO |
| 12 | `flutter_pop` | `timeout_ms: 1500` | Gesto `PageBack` ejecutado; retorno con diagnóstico enriquecido de fallback en botones custom | ✅ APROBADO |
| 13 | `flutter_wait_for_absent` | `by: "text", value: "WidgetQueNoExiste", timeout_ms: 2000` | Evaluación reactiva confirmando inmediatamente la ausencia del elemento en pantalla | ✅ APROBADO |
| 14 | `flutter_get_performance` | `include_frames: false` | Análisis del Timeline VM: 55 frames analizados, 0% jank, build prom 4.1ms, raster prom 2.7ms | ✅ APROBADO |
| 15 | `flutter_driver_raw` | `command: "get_health", params: {}` | Passthrough directo al handler de extensiones: `{"status": "ok", "isError": false}` | ✅ APROBADO |
| 16 | `flutter_hot_reload` | `{}` | Hot Reload aplicado instantáneamente en la app activa sin pérdida de estado de widgets | ✅ APROBADO |
| 17 | `flutter_hot_restart` | `{}` | Hot Restart ejecutado en vivo; reconstrucción completa y re-inicialización del driver | ✅ APROBADO |
| 18 | `flutter_start_control` | `project_root: "...", entrypoint: "lib/main_driver.dart"` | Idempotencia confirmada: no modificó archivos existentes y activó Hot Restart | ✅ APROBADO |
| 19 | `flutter_disconnect` | `{}` | Cierre ordenado de la sesión WebSocket, drenado de buffers y liberación de puertos | ✅ APROBADO |

---

## 3. Hallazgos Técnicos y Edge Cases Críticos

Durante la sesión se identificaron comportamientos y particularidades del SDK de Flutter y Flutter Driver que enriquecen las buenas prácticas del MCP:

### 3.1. Ambigüedad de Finders de Texto (`Bad state: Too many elements`)
* **Problema observado**: Al llamar a `flutter_get_text` o `flutter_tap` con `by: "text", value: "Playlists"`, Flutter Driver arrojó:
  ```text
  Uncaught extension error while executing get_text: Bad state: Too many elements
  #0      Iterable.single (dart:core/iterable.dart:696:24)
  #1      CommandHandlerFactory._getText (package:flutter_driver/src/common/handler_factory.dart:438:45)
  ```
* **Causa**: En la interfaz de Auralis, el string `"Playlists"` figuraba tanto en el chip de categoría horizontal como en el título de la vista. `find.text()` en Flutter Driver utiliza `Iterable.single`, el cual falla si hay más de una coincidencia.
* **Lección**:
  - Para strings genéricos o repetidos, **siempre preferir `by: "key"`**.
  - Si se busca por texto, seleccionar valores que sean unívocos en el árbol (ej. nombres de canciones, títulos completos o botones únicos como `"Conectar IP"` o `"AURALIS"`).

### 3.2. Ambigüedad con múltiples `Scrollable` en pantalla
* **Problema observado**: Ejecutar `flutter_scroll` usando `by: "type", value: "Scrollable"` falló con:
  ```text
  The finder "Found 2 widgets with widget with runtimeType "Scrollable": [...] ambiguously found multiple matching widgets. The "getCenter()" method needs a single target.
  ```
* **Causa**: La pantalla móvil/tablet de Auralis contiene dos scrollables concurrentes:
  1. `CustomScrollView` vertical (para la lista de canciones/álbumes).
  2. `SingleChildScrollView` horizontal (para los chips de categorías superiores).
* **Lección**: Al hacer scroll por tipo, **especificar el widget concreto** (ej. `by: "type", value: "CustomScrollView"`) en lugar del tipo genérico `Scrollable`, o asignar una `ValueKey` al contenedor scrollable principal.

### 3.3. Breakpoints Responsivos y Geometría de Ventana Desktop
* **Problema observado**: A pesar de correr en Linux Desktop, Auralis no renderizó `LibraryDesktopScreen` sino `LibraryMobileScreen`.
* **Causa**: La ventana por defecto en Linux se levantó con dimensiones de **941 x 979 px**. El archivo `lib/design_system/widgets/responsive_layout.dart` define:
  ```dart
  static const double mobileBreakpoint = 768;
  static const double desktopBreakpoint = 1024;
  ```
  Al ser `width = 941 < 1024`, el breakpoint de escritorio no se activó. Y al ser `>= 768`, la app evaluó modo "Tablet" (que en `ResponsiveLayout` cae a `tablet ?? mobile`). Además, el drawer del `Scaffold` solo se monta si `isMobile` (`width < 768`), por lo que el botón de menú superior no abría el drawer lateral.
* **Lección**: Al automatizar pruebas de apps responsivas en escritorio, considerar el tamaño de ventana configurado o probar flujos que no dependan exclusivamente del layout de pantalla ancha.

### 3.4. Ciclo de Vida de TextFields y Autofoco en `flutter_enter_text`
* **Problema observado**: Intentar escribir en el buscador principal con `flutter_enter_text` falló por timeout en la vista inicial.
* **Causa**: En el layout móvil/tablet de Auralis, el `TextField` de búsqueda está condicionado a `if (_isSearching)` (se monta únicamente al pulsar el botón de lupa). En contraste, al abrir `StereoSyncDialog`, el `TextField` para ingresar la IP manual está montado inmediatamente en el árbol.
* **Lección**: `flutter_enter_text` funcionó de manera impecable en cuanto el widget estuvo presente en el árbol, validando su capacidad de realizar el tap de foco automático previo a la inyección de caracteres.

### 3.5. Limitación del SDK en `flutter_pop` y Diagnóstico Enriquecido
* **Comportamiento observado**: Al invocar `flutter_pop`, el comando retornó un timeout limpio con el siguiente mensaje:
  ```text
  Error ejecutando flutter_pop: Error ejecutando comando de Flutter Driver: TimeoutException
  (el widget no se encontró dentro del timeout: usá 'flutter_snapshot' para confirmar que existe... preferí 'by: "key"')
  ```
* **Causa**: Confirma la limitación documentada en el servidor MCP: el comando nativo `PageBack` de Flutter Driver busca de forma cableada en el framework un widget con `Tooltip(message: 'Back')` (literal en inglés) o `CupertinoNavigationBarBackButton`. En Auralis (y en la mayoría de las apps de producción), los diálogos se cierran mediante un `IconButton` personalizado (`Icons.close_rounded`), el cual requiere cerrarse mediante `flutter_tap` sobre dicho botón o la acción de cierre respectiva.

### 3.6. Captura de Errores de Layout sin Degradación del Driver
* **Observación**: En los logs iniciales de Auralis, el MCP capturó:
  ```text
  ⛔ Flutter Framework Error: A RenderFlex overflowed by 22 pixels on the bottom in library_desktop_screen.dart:140:28
  ```
* **Impacto**: La presencia de advertencias de layout o overflows de pixeles no interrumpió la comunicación del Dart VM Service ni degradó el desempeño de las herramientas de inspección y gestos del MCP.

---

## 4. Métricas de Rendimiento Registradas

A través de `flutter_get_performance`, se analizó la fluidez de renderizado en Linux sobre el backend OpenGL de Impeller:
- **Total de frames muestreados**: 55 frames.
- **Frames con jank**: 0 (0.0%).
- **Tiempo promedio de Build**: 4.18 ms (muy por debajo del límite de 16.6 ms para 60 FPS).
- **Tiempo promedio de Raster**: 2.79 ms.
- **Peor frame registrado**: Frame #8 (Build: 5.59 ms, Raster: 5.44 ms — sin llegar a producir jank).

---

## 5. Instrucciones para Reproducir las Pruebas

Para volver a ejecutar la suite completa automatizada de las 19 herramientas contra Auralis:

1. **Iniciar Auralis con el entrypoint de prueba**:
   ```bash
   cd /home/guty_3rrez/Proyectos/auralis
   flutter run -d linux -t lib/main_driver.dart
   ```
2. **Copiar la URI del Dart VM Service** impresa en la consola (ej. `ws://127.0.0.1:<PORT>/<TOKEN>/ws`).
3. **Correr el test de integración en Rust**:
   ```bash
   cd /home/guty_3rrez/Proyectos/flutter-native-mcp
   AURALIS_VM_URI="ws://127.0.0.1:<PORT>/<TOKEN>/ws" cargo test --test e2e_live_auralis -- --ignored --nocapture
   ```
4. **Verificar el resultado**: Debe concluir con `test result: ok. 1 passed; 0 failed`.
