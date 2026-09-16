# Software Requirements Specification (SRS)
## Project: Flutter Native MCP (`flutter-native-mcp`)
*Model Context Protocol (MCP) Server for Native Flutter UI Automation, Inspection & Testing*

**Document Version:** 0.1.0-draft  
**Status:** In Review / Iterative Formulation  
**Authors:** guty_3rrez & Antigravity  
**Date:** September 2026  

---

## 1. Introducción y Propósito

### 1.1 Propósito
Este documento define los requisitos funcionales, no funcionales y la arquitectura técnica del proyecto **`flutter-native-mcp`**.
El objetivo principal es permitir que agentes inteligentes de IA (LLMs) y herramientas de automatización interactúen de forma nativa, bidireccional y eficiente con aplicaciones desarrolladas en **Flutter** (especialmente en entornos de escritorio como **Linux Desktop**, macOS, Windows y dispositivos móviles como Android/iOS) sin depender de navegadores web ni wrappers externos como Microsoft Playwright.

### 1.2 Declaración del Problema
Actualmente, la automatización de interfaces asistida por agentes para frameworks cross-platform suele recurrir a:
1. **Playwright sobre Flutter Web**:
   - **Limitación crítica**: Flutter Web (con renderizadores CanvasKit o Skwasm/WebAssembly) no produce elementos HTML semánticos en el DOM (`<button>`, `<div>`, `<input>`); en su lugar, pinta directamente vectores y bitmaps en un elemento `<canvas>`.
   - **Consecuencia**: Los selectores CSS y XPath fallan, el árbol de accesibilidad web suele presentar desincronizaciones de coordenadas y se desperdician recursos levantando Chromium para probar una aplicación concebida para escritorio o móvil.
2. **Control por Visión de Pantalla (OS-level mouse/keyboard automation)**:
   - Depende de OCR, coordenadas de pantalla absolutas, resolución de monitor y captura de pantalla global.
   - Es altamente propenso a fallos, ciego a los estados internos del framework y sensible al movimiento de ventanas.

### 1.3 Visión de la Solución
Construir un servidor MCP que se conecte directamente al **Dart VM Service Protocol** y a las **Flutter Service Extensions** de la aplicación en ejecución. Esto permite:
- **Introspección real**: Conocer el árbol de widgets, árboles semánticos (`SemanticsNode`), llaves (`Key`) y estados internos en memoria Dart.
- **Inyección nativa de gestos**: Disparar `PointerDownEvent`, `PointerUpEvent` y simulaciones de teclado directamente a través del `GestureBinding` y `ServicesBinding`.
- **Sincronización de reloj y frames**: Ejecutar `pump()` y `pumpAndSettle()` respetando el bucle de animaciones del framework.
- **Eficiencia de tokens**: Generar representaciones textuales compactas (UI Snapshots) para consumo de LLMs con costo de contexto mínimo.

---

## 2. Fundamentos Técnicos del Framework Flutter

Para garantizar el cumplimiento de los requerimientos, el sistema se basa en los componentes estructurales de Flutter:

```
+-----------------------------------------------------------------------------------+
|                                Dart VM Service                                    |
|         (JSON-RPC 2.0 sobre WebSocket / Dart Tooling Daemon [DTD])                |
+-----------------------------------------------------------------------------------+
                                          |
+-----------------------------------------------------------------------------------+
|                           Flutter Service Extensions                              |
|   ext.flutter.driver  |  ext.flutter.inspector  |  ext.flutter.renderFrameDone    |
+-----------------------------------------------------------------------------------+
                                          |
+-----------------------------------------------------------------------------------+
|                             Flutter Framework Layer                               |
|                                                                                   |
|  * WidgetsBinding: Element tree, State, BuildOwner                                |
|  * RendererBinding: RenderObject tree, PipelineOwner, Layout, Paint               |
|  * SchedulerBinding: Frame callbacks, Tickers, Animaciones                        |
|  * GestureBinding: Hit testing, PointerEvent dispatching                          |
|  * SemanticsBinding: Accessibility tree (SemanticsNode, roles, flags, actions)    |
+-----------------------------------------------------------------------------------+
                                          |
+-----------------------------------------------------------------------------------+
|                        Flutter Engine (C++ / Impeller / Skia)                     |
|       Ventana Nativa (GTK en Linux, Cocoa en macOS, Win32, Android Surface)       |
+-----------------------------------------------------------------------------------+
```

### 2.1 El Mecanismo de `pump`
- En tiempo de ejecución normal, el `SchedulerBinding` procesa frames empujados por la señal de hardware VSYNC.
- En modo automatizado, el sistema de control puede forzar la rasterización y avance de animaciones llamando a `handleBeginFrame()` y `handleDrawFrame()`.
- La operación `pumpAndSettle` itera sobre el despachador de tareas hasta que:
  1. No existan animaciones activas (Tickers sin detener).
  2. La cola de microtareas esté vacía.
  3. No haya timers pendientes inmediatos.

---

## 3. Alcance y Plataformas Objetivo

### 3.1 Plataformas Soportadas
- **Linux Desktop (GTK)**: Prioridad inicial de desarrollo (desarrollo local del agente).
- **macOS Desktop**: Soporte nativo equivalente.
- **Windows Desktop**: Soporte nativo equivalente.
- **Android**: Emuladores y dispositivos físicos vía ADB + port forwarding.
- **iOS**: Simuladores y dispositivos vía `ios-deploy` / túnel de depuración.

### 3.2 Modos de Conexión de la Aplicación
1. **Modo Zero-Touch (Inspector VM Service)**:
   - Se conecta a cualquier app Flutter compilada en modo `debug` o `profile` vía su URI de VM Service (`ws://127.0.0.1:<port>/<token>/ws`).
   - Requiere únicamente que la app esté corriendo con observatorio activo.
2. **Modo Driver (`enableFlutterDriverExtension`)**:
   - Para aplicaciones que añaden `enableFlutterDriverExtension()` en su punto de entrada (`main.dart` o `main_test.dart`).
   - Permite ejecución directa de comandos primitivos de tapping, texto y desplazamiento con mayor fidelidad.
3. **Modo Companion Package (Futura optimización)**:
   - Un paquete ligero `flutter_agent_mcp` añadido como `dev_dependency` para generar snapshots semánticos ultraligeros y enriquecidos en un solo salto de red.

---

## 4. Requerimientos Funcionales (FR)

### FR-1: Gestión de Ciclo de Vida y Conexión
- **FR-1.1**: El servidor MCP debe permitir conectarse a una instancia activa de Flutter proporcionando la URI del Dart VM Service (`ws://...` o `http://...`).
- **FR-1.2**: El servidor MCP debe ser capaz de autodescubrir el Isolate principal de Flutter que aloja el `WidgetsBinding`.
- **FR-1.3**: El servidor MCP debe proveer una herramienta opcional para compilar y arrancar la aplicación en segundo plano (`flutter_launch_app` con argumentos de target y dispositivo, ej. `-d linux`).
- **FR-1.4**: El servidor MCP debe gestionar la desconexión limpia sin abortar el proceso de la aplicación a menos que se solicite explícitamente (`flutter_disconnect`, `flutter_terminate_app`).

### FR-2: Introspección y Snapshot Semántico para LLMs (Core)
- **FR-2.1 (UI Snapshot)**: Debe exponer la herramienta `flutter_snapshot` que devuelva una representación jerárquica limpia, concisa y estructurada (YAML o JSON) del estado visual actual.
- **FR-2.2 (Tree Pruning / Poda del Árbol)**:
  - Debe descartar automáticamente nodos internos de maquetación no interactivos (como `Padding`, `SizedBox`, `ConstrainedBox`, `ColoredBox`, `RepaintBoundary`, etc.).
  - Debe retener únicamente:
    - Elementos interactivos (`Button`, `InkWell`, `GestureDetector`, `Switch`, `Checkbox`, `Slider`, `Dropdown`).
    - Elementos informativos (`Text`, `RichText`, `Icon`, `Image`).
    - Contenedores semánticos (`Scaffold`, `AppBar`, `Card`, `Dialog`, `ListView`, `ScrollView`).
    - Nodos con `Key` explícita (`ValueKey`, `Key`).
- **FR-2.3 (Atributos de Nodo)**:
  - Cada elemento del snapshot debe incluir: `type`, `key` (si existe), `text`/`value` (si aplica), `semanticsLabel` / `tooltip`, `enabled` (booleano), `focused` (booleano) y `bounds` (rectángulo de pantalla relativo o absoluto).
- **FR-2.4 (Límite de Tokens)**: El snapshot promedio de una pantalla estándar no debe exceder los 1.500 tokens para garantizar compatibilidad y velocidad con LLMs.

### FR-3: Selectores e Identificadores (Finders)
El sistema debe permitir referenciar widgets mediante un selector polimórfico flexible:
- **Por Key**: `{"by": "key", "value": "login_button"}`
- **Por Texto**: `{"by": "text", "value": "Iniciar Sesión", "exact": true|false}`
- **Por Semantics Label / Tooltip**: `{"by": "tooltip", "value": "Buscar"}`
- **Por Tipo de Widget**: `{"by": "type", "value": "ElevatedButton"}`
- **Por Coordenadas**: `{"by": "coordinates", "x": 350, "y": 420}`
- **Por Relación Jerárquica**: `{"by": "descendant", "of": {...}, "matching": {...}}`

### FR-4: Interacción y Gestos
- **FR-4.1 (`flutter_tap`)**:
  - Resuelve el elemento objetivo, calcula su centro geométrico global y despacha la secuencia `PointerDownEvent` -> `pump` -> `PointerUpEvent`.
- **FR-4.2 (`flutter_enter_text`)**:
  - Selecciona un campo de texto (`TextField`, `TextFormField`), le transfiere el foco del framework y actualiza su valor (`TextEditingValue`) o simula pulsaciones de teclado.
- **FR-4.3 (`flutter_clear_text`)**:
  - Limpia el contenido de un campo de texto enfocado.
- **FR-4.4 (`flutter_scroll`)**:
  - Realiza un desplazamiento incremental (`dx`, `dy`) con una duración y frecuencia determinada sobre un contenedor scrollable.
- **FR-4.5 (`flutter_scroll_until_visible`)**:
  - Ejecuta un bucle de scroll progresivo hasta que el widget destino esté montado en el viewport y sea visible.
- **FR-4.6 (`flutter_send_key_action`)**:
  - Envía acciones de teclado virtuales (`TextInputAction.done`, `newline`, etc.) o eventos de teclas físicas (`KeyDownEvent`, `KeyUpEvent`).

### FR-5: Sincronización y Espera de Estados
- **FR-5.1 (`flutter_wait_for`)**:
  - Espera de forma no bloqueante a que aparezca un elemento que coincida con el selector especificado, con un timeout configurable (default: 5000ms).
- **FR-5.2 (`flutter_wait_for_absent`)**:
  - Espera a que un elemento desaparezca (útil para verificar cierre de loaders o modales).
- **FR-5.3 (`flutter_pump_and_settle`)**:
  - Espera a que todas las animaciones y tareas pendientes terminen antes de proseguir. Incluye guardia contra animaciones continuas (ej. `CircularProgressIndicator` en bucle).

### FR-6: Inspección Visual y Diagnósticos
- **FR-6.1 (`flutter_screenshot`)**:
  - Captura el búfer de renderizado del motor Flutter (utilizando `RenderView.compositeFrame` o la extensión de captura de DevTools/Driver) y devuelve la imagen en Base64 o la almacena en disco para inspección de modelos multimodales.
- **FR-6.2 (`flutter_get_errors`)**:
  - Recupera los errores y excepciones no controladas registradas en el `FlutterError.onError` de la app (ej. `RenderFlex overflowed by xx pixels`).

### FR-7: Ciclo de Desarrollo Agéntico (Live Dev Loop)
- **FR-7.1 (`flutter_hot_reload`)**:
  - Invoca la recarga en caliente de Dart VM en milisegundos tras una modificación de archivo efectuada por el agente.
- **FR-7.2 (`flutter_hot_restart`)**:
  - Reinicia el estado de la app sin reiniciar el proceso del sistema operativo ni la conexión del socket.

---

## 5. Requerimientos No Funcionales (NFR)

### NFR-1: Rendimiento y Latencia
- El tiempo de respuesta de comandos de acción simple (`tap`, `enter_text`) no debe superar los **150 ms** en localhost (excluyendo el tiempo propio de animación de la app).
- El tiempo de generación del `flutter_snapshot` procesado no debe superar los **300 ms**.

### NFR-2: Consumo de Tokens y Ergonomía para LLMs
- El snapshot debe ser directamente comprensible para modelos con razonamiento visual y de texto, usando indentación semántica simple sin anidaciones redundantes de 20 niveles.

### NFR-3: Estabilidad y Tolerancia a Fallos
- Si una app se cierra inesperadamente o entra en un bucle infinito, el servidor MCP debe emitir un error de timeout limpio sin colapsar el proceso del servidor MCP ni bloquear al agente.

---

## 6. Catálogo de Herramientas MCP Propuestas

| Nombre de Herramienta | Parámetros Principales | Descripción |
| :--- | :--- | :--- |
| `flutter_connect` | `vmServiceUri`, `appTarget` | Establece conexión con el Dart VM Service de la app. |
| `flutter_disconnect` | *(ninguno)* | Cierra la sesión activa con la app. |
| `flutter_snapshot` | `summaryOnly: bool`, `maxDepth: int` | Retorna el árbol semántico podado y listo para el LLM. |
| `flutter_tap` | `finder: FinderObject`, `timeoutMs: int` | Simula un tap en el widget objetivo y espera el frame. |
| `flutter_enter_text` | `finder: FinderObject`, `text: string` | Escribe texto en un campo interactivo. |
| `flutter_scroll` | `finder: FinderObject`, `dx: num`, `dy: num` | Desplaza el contenido de una lista o vista scrollable. |
| `flutter_wait_for` | `finder: FinderObject`, `timeoutMs: int` | Aguarda la presencia de un elemento en el árbol. |
| `flutter_screenshot` | `savePath?: string` | Captura la pantalla actual de la app como imagen. |
| `flutter_hot_reload` | *(ninguno)* | Ejecuta Hot Reload en la sesión activa. |
| `flutter_get_diagnostics`| `type: "errors" \| "logs"` | Retorna excepciones de renderizado o logs recientes. |

---

## 7. Decisiones de Arquitectura Adoptadas (Iteración 1)

1. **Lenguaje del Servidor MCP: Rust** (Decisión Adoptada)
   - **SDK de MCP**: Crate oficial **`rmcp`** (v3.4.0, mantenido por la organización oficial de *Model Context Protocol*).
     - Provee macros directas `#[tool]`, integración asíncrona con `tokio`, esquemas JSON con `schemars` y transporte stdio.
   - **Cliente WebSocket / VM Service**: `tokio-tungstenite` + `serde` / `serde_json` para gestionar el protocolo JSON-RPC 2.0 con el Dart VM Service.
   - **Ventajas críticas para este caso de uso**:
     - **Binario standalone sin dependencias**: Se compila a un único ejecutable sin requerir Node.js ni el SDK de Dart en el host que ejecuta el MCP.
     - **Poda ultrarrápida en memoria**: El árbol de diagnósticos de Flutter puede pesar megabytes en JSON crudo; Rust lo procesa y poda a snapshot conciso en menos de 2ms sin sobrecarga de recolección de basura (GC).
     - **Huella de memoria mínima**: ~5MB a 10MB de RAM frente a los 80MB-150MB de Node o Dart JIT.

2. **Grado de Invasión en la App Objetivo**:
   - Prioridad 1: **Modo Dual**:
     - *Modo Zero-Touch*: Capaz de inspeccionar árbol (`ext.flutter.inspector`) en cualquier app debug sin tocar su código.
     - *Modo Driver*: Si la app tiene `enableFlutterDriverExtension()`, habilita inyección completa de gestos (`tap`, `enter_text`, `scroll`).

---

## 8. Roadmap de Iteración

- [ ] **Hito 1**: Congelar decisiones de arquitectura (Lenguaje del MCP y Modo de conexión primario).
- [ ] **Hito 2**: Spike de conexión y extracción de árbol de widgets podado (`flutter_snapshot`).
- [ ] **Hito 3**: Implementación de inyección de gestos básicos (`tap`, `enter_text`) y `pump`.
- [ ] **Hito 4**: Integración de captura visual (`screenshot`) y captura de errores de layout.
- [ ] **Hito 5**: Empaquetado, documentación de uso y registro en configuración local de MCP.
