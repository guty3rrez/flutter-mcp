use crate::application::ports::inbound::FlutterAppService;
use crate::domain::entities::{Finder, LogFilter};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Formatea un timestamp en millis-desde-época como hora del día UTC `HH:MM:SS.mmm`, sin
/// depender de una crate de fechas: suficiente precisión para correlacionar logs/errores
/// entre sí durante una sesión de depuración.
fn format_timestamp_ms(ts: i64) -> String {
    if ts <= 0 {
        return "??:??:??.???".to_string();
    }
    let millis = ts as u64;
    let secs_of_day = (millis / 1000) % 86_400;
    let ms = millis % 1000;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

fn parse_finder(by: &str, value: String) -> std::result::Result<Finder, String> {
    match by {
        "key" => Ok(Finder::by_key(value)),
        "text" => Ok(Finder::by_text(value, true)),
        "tooltip" => Ok(Finder::Tooltip(value)),
        "type" => Ok(Finder::by_type(value)),
        "semantics" => Ok(Finder::SemanticsLabel(value)),
        other => Err(format!(
            "Criterio de búsqueda desconocido '{other}'. Use 'key', 'text', 'tooltip', 'type' o 'semantics'"
        )),
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConnectParams {
    #[schemars(
        description = "URI del Dart VM Service de la app Flutter (ej: ws://127.0.0.1:45678/ws)"
    )]
    pub uri: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TapParams {
    #[schemars(
        description = "Criterio de búsqueda: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar (nombre de la key, texto exacto, tooltip, etc.)")]
    pub value: String,
    #[schemars(
        description = "Tiempo máximo de espera en milisegundos para el comando de Flutter Driver. Si se omite, se usa la heurística automática de pre-chequeo (800ms si el finder no matchea nada en el árbol actual, 5000ms si matchea o no aplica)."
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct FlutterPopParams {
    #[schemars(
        description = "Tiempo máximo de espera en milisegundos para el gesto de retroceso (opcional; por defecto usa la heurística automática, igual que flutter_tap)"
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EnterTextParams {
    #[schemars(
        description = "Criterio de búsqueda: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
    #[schemars(description = "Texto a ingresar en el campo")]
    pub text: String,
    #[schemars(
        description = "Tiempo máximo de espera en milisegundos para el tap de foco previo al ingreso de texto. Si se omite, se usa la heurística automática de pre-chequeo."
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetTextParams {
    #[schemars(
        description = "Criterio de búsqueda: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScrollParams {
    #[schemars(
        description = "Criterio de búsqueda del contenedor scrollable: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
    #[schemars(description = "Desplazamiento horizontal en píxeles")]
    pub dx: f64,
    #[schemars(
        description = "Desplazamiento vertical en píxeles (negativo hacia abajo, positivo hacia arriba)"
    )]
    pub dy: f64,
    #[schemars(description = "Duración del scroll en milisegundos (por defecto 300)")]
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[schemars(description = "Frecuencia de muestreo del gesto en Hz (por defecto 60)")]
    #[serde(default)]
    pub frequency: Option<u32>,
    #[schemars(
        description = "Tiempo máximo de espera en milisegundos para el comando de Flutter Driver. Si se omite, se usa la heurística automática de pre-chequeo."
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScrollIntoViewParams {
    #[schemars(
        description = "Criterio de búsqueda del widget a hacer visible: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
    #[schemars(
        description = "Alineación en el viewport de 0.0 (inicio) a 1.0 (final). Por defecto 0.0"
    )]
    #[serde(default)]
    pub alignment: Option<f64>,
    #[schemars(
        description = "Tiempo máximo de espera en milisegundos para el comando de Flutter Driver. Si se omite, se usa la heurística automática de pre-chequeo."
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WaitForParams {
    #[schemars(
        description = "Criterio de búsqueda: 'key', 'text', 'tooltip', 'type' o 'semantics'"
    )]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
    #[schemars(description = "Tiempo máximo de espera en milisegundos (por defecto 5000)")]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ScreenshotParams {
    #[schemars(
        description = "Ruta opcional en el sistema de archivos donde guardar el archivo PNG de la captura de pantalla"
    )]
    #[serde(default)]
    pub save_path: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct GetLogsParams {
    #[schemars(description = "Filtrar líneas que contengan este texto (case-insensitive)")]
    #[serde(default)]
    pub filter: Option<String>,
    #[schemars(
        description = "Restringir a un origen: 'stdout', 'stderr' o 'logging' (dart:developer.log)"
    )]
    #[serde(default)]
    pub source: Option<String>,
    #[schemars(
        description = "Máximo de líneas a devolver, las más recientes primero (por defecto 100, para no inundar el contexto)"
    )]
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct GetErrorsParams {
    #[schemars(
        description = "Máximo de errores a devolver, los más recientes primero (por defecto 20)"
    )]
    #[serde(default)]
    pub limit: Option<u32>,
    #[schemars(
        description = "Si es true, intenta además activar captura precisa vía pausa en excepción (setExceptionPauseMode/PauseException). Advertencia validada contra una app Flutter real: en ese entorno el engine no disparó PauseException para una excepción async no capturada (probablemente porque el propio engine 'maneja' la excepción antes de que el debugger la considere no-manejada) — no asumir que este modo captura lo que el pasivo se pierde. Por defecto false."
    )]
    #[serde(default)]
    pub precise: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct StartControlParams {
    #[schemars(
        description = "Directorio raíz del proyecto Flutter (el que contiene pubspec.yaml). Por defecto el directorio de trabajo actual ('.')"
    )]
    #[serde(default)]
    pub project_root: Option<String>,
    #[schemars(
        description = "Ruta del entrypoint a parchar, relativa a project_root. Por defecto 'lib/main.dart'"
    )]
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[schemars(
        description = "Si es true, revierte el archivo del entrypoint a su contenido original en disco inmediatamente después del Hot Restart (el registro de la extensión ya quedó activo en el binding en memoria de la app, independiente del archivo). Por defecto false: el cambio queda en el archivo para sobrevivir a futuros Hot Reload/Restart de la sesión."
    )]
    #[serde(default)]
    pub revert_after_restart: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct GetPerformanceParams {
    #[schemars(
        description = "Acotar el análisis a los últimos N milisegundos de timeline (por defecto: todo el buffer acumulado)"
    )]
    #[serde(default)]
    pub window_ms: Option<u64>,
    #[schemars(
        description = "Si es true, incluye el detalle por frame además del resumen agregado (por defecto false)"
    )]
    #[serde(default)]
    pub include_frames: Option<bool>,
}

fn default_driver_raw_params() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DriverRawParams {
    #[schemars(
        description = "Nombre del comando de Flutter Driver a ejecutar tal cual (ej. 'set_frame_sync', 'set_text_entry_emulation', 'send_text_input_action', o un comando de una FlutterDriverExtension personalizada de la app)"
    )]
    pub command: String,
    #[schemars(
        description = "Parámetros del comando como objeto JSON. No incluir 'command' ni 'isolateId': el servidor los agrega automáticamente. Los valores deben ir como string, como exige el protocolo Flutter Driver (ej. {\"enabled\": \"true\"})"
    )]
    #[serde(default = "default_driver_raw_params")]
    pub params: serde_json::Value,
}

#[derive(Clone)]
pub struct FlutterMcpServer {
    app_service: Arc<dyn FlutterAppService>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl FlutterMcpServer {
    pub fn new(app_service: Arc<dyn FlutterAppService>) -> Self {
        Self {
            app_service,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Conectar con el Dart VM Service de una app Flutter en ejecución")]
    async fn flutter_connect(
        &self,
        Parameters(params): Parameters<ConnectParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.connect(&params.uri).await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "Conectado exitosamente a la app Flutter en {}",
                params.uri
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error de conexión: {e}"
            ))])),
        }
    }

    #[tool(description = "Desconectar la sesión activa del Dart VM Service")]
    async fn flutter_disconnect(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.disconnect().await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Desconectado exitosamente de la app Flutter",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error al desconectar: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Obtener el árbol de UI simplificado y podado (UI Snapshot) de la app Flutter actual"
    )]
    async fn flutter_snapshot(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.get_pruned_snapshot().await {
            Ok(snapshot) => {
                let json_repr = serde_json::to_string_pretty(&snapshot)
                    .unwrap_or_else(|_| "Error al serializar snapshot".into());
                Ok(CallToolResult::success(vec![ContentBlock::text(json_repr)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error obteniendo snapshot: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Hacer tap (clic) sobre un widget localizado por Key, texto, tooltip, tipo o semantics"
    )]
    async fn flutter_tap(
        &self,
        Parameters(params): Parameters<TapParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        match self.app_service.tap(finder, params.timeout_ms).await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Tap ejecutado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ejecutando tap: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Ejecutar el gesto de retroceso estándar de Flutter Driver ('PageBack'): intenta primero el botón con tooltip 'Back' y, si no existe, cae a los widgets nativos de retroceso (CupertinoNavigationBarBackButton/BackButtonIcon) de forma independiente del idioma de la app. No requiere localizar el widget manualmente ni una extensión Dart personalizada -- preferí esta tool sobre flutter_tap(by: 'tooltip', value: 'Back') porque esa combinación no tiene ese fallback por tipo de widget."
    )]
    async fn flutter_pop(
        &self,
        Parameters(params): Parameters<FlutterPopParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self
            .app_service
            .tap(Finder::PageBack, params.timeout_ms)
            .await
        {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Retroceso (pop) ejecutado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ejecutando flutter_pop: {e}"
            ))])),
        }
    }

    #[tool(description = "Ingresar texto en un campo interactivo (TextField/TextFormField)")]
    async fn flutter_enter_text(
        &self,
        Parameters(params): Parameters<EnterTextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        match self
            .app_service
            .enter_text(finder, params.text, params.timeout_ms)
            .await
        {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Texto ingresado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ingresando texto: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Obtener el texto contenido dentro de un widget (Text, EditableText, etc.)"
    )]
    async fn flutter_get_text(
        &self,
        Parameters(params): Parameters<GetTextParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        match self.app_service.get_text(finder).await {
            Ok(text) => Ok(CallToolResult::success(vec![ContentBlock::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error obteniendo texto: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Hacer scroll en un contenedor scrollable (ListView, CustomScrollView, etc.)"
    )]
    async fn flutter_scroll(
        &self,
        Parameters(params): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let duration_ms = params.duration_ms.unwrap_or(300);
        let frequency = params.frequency.unwrap_or(60);

        match self
            .app_service
            .scroll(
                finder,
                params.dx,
                params.dy,
                duration_ms,
                frequency,
                params.timeout_ms,
            )
            .await
        {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Scroll ejecutado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ejecutando scroll: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Desplazar un contenedor hasta que el widget objetivo sea completamente visible en pantalla"
    )]
    async fn flutter_scroll_into_view(
        &self,
        Parameters(params): Parameters<ScrollIntoViewParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let alignment = params.alignment.unwrap_or(0.0);

        match self
            .app_service
            .scroll_into_view(finder, alignment, params.timeout_ms)
            .await
        {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Widget desplazado hacia la vista con éxito",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error en scroll_into_view: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Esperar a que un widget aparezca en el árbol de widgets (espera asíncrona)"
    )]
    async fn flutter_wait_for(
        &self,
        Parameters(params): Parameters<WaitForParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let timeout_ms = params.timeout_ms.unwrap_or(5000);

        match self.app_service.wait_for(finder, timeout_ms).await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Widget localizado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error esperando widget: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Esperar a que un widget desaparezca de la pantalla (ej. loaders, diálogos)"
    )]
    async fn flutter_wait_for_absent(
        &self,
        Parameters(params): Parameters<WaitForParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match parse_finder(&params.by, params.value) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };

        let timeout_ms = params.timeout_ms.unwrap_or(5000);

        match self.app_service.wait_for_absent(finder, timeout_ms).await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Widget ausente confirmado",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error esperando ausencia del widget: {e}"
            ))])),
        }
    }

    #[tool(description = "Disparar Hot Reload en la aplicación Flutter activa")]
    async fn flutter_hot_reload(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.hot_reload().await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Hot Reload ejecutado con éxito",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error en Hot Reload: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Disparar Hot Restart / Reassemble completo en la aplicación Flutter activa"
    )]
    async fn flutter_hot_restart(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.hot_restart().await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Hot Restart ejecutado con éxito",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error en Hot Restart: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Inyectar en caliente la capacidad de control (Flutter Driver) en el entrypoint principal (lib/main.dart por defecto) de una app Flutter YA conectada que fue lanzada con su entrypoint normal (sin main_driver.dart), y disparar un Hot Restart para activarla. Requisito: 'flutter_driver' debe ser ya una dependencia resuelta del proyecto (pubspec.lock). Si no lo es, esta tool la agrega a pubspec.yaml (dev_dependencies) y se detiene ahí: hace falta correr 'flutter pub get' y reiniciar por completo el proceso 'flutter run' (un Hot Restart no alcanza para resolver una dependencia nueva), y volver a llamar a esta tool. Es idempotente: si el entrypoint ya tiene Flutter Driver habilitado, solo dispara el Hot Restart."
    )]
    async fn flutter_start_control(
        &self,
        Parameters(params): Parameters<StartControlParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let project_root = params.project_root.unwrap_or_else(|| ".".into());
        let entrypoint = params.entrypoint.unwrap_or_else(|| "lib/main.dart".into());
        let revert = params.revert_after_restart.unwrap_or(false);

        match self
            .app_service
            .start_control(project_root, entrypoint, revert)
            .await
        {
            Ok(outcome) if outcome.pubspec_updated => {
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "'flutter_driver' no era una dependencia resuelta del proyecto: se agregó a pubspec.yaml (dev_dependencies). Corré 'flutter pub get' y reiniciá por completo 'flutter run' (un Hot Restart no basta para una dependencia nueva), luego volvé a llamar a flutter_start_control. Entrypoint objetivo: {}",
                    outcome.entrypoint_path
                ))]))
            }
            Ok(outcome) if outcome.already_enabled => {
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "El entrypoint '{}' ya tenía Flutter Driver habilitado. Hot Restart ejecutado para asegurar que la extensión esté activa.",
                    outcome.entrypoint_path
                ))]))
            }
            Ok(outcome) => {
                let revert_note = if outcome.reverted {
                    " El archivo en disco fue revertido a su versión original tras el restart."
                } else {
                    ""
                };
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "Flutter Driver inyectado en '{}' y Hot Restart ejecutado con éxito -- la app ahora acepta comandos de control.{revert_note}",
                    outcome.entrypoint_path
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error activando control de la app: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Tomar una captura de pantalla (screenshot) de la interfaz de la app Flutter"
    )]
    async fn flutter_screenshot(
        &self,
        Parameters(params): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.take_screenshot().await {
            Ok(bytes) => {
                let mut msg = format!(
                    "Captura de pantalla tomada con éxito ({} bytes)",
                    bytes.len()
                );
                if let Some(path) = params.save_path {
                    match std::fs::write(&path, &bytes) {
                        Ok(_) => {
                            msg.push_str(&format!(". Guardada en: {path}"));
                        }
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                                "Captura obtenida pero error guardando en '{path}': {e}"
                            ))]));
                        }
                    }
                }
                Ok(CallToolResult::success(vec![ContentBlock::text(msg)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error tomando screenshot: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Leer los logs (stdout/stderr/dart:developer.log) acumulados pasivamente desde que se conectó la app. Por defecto devuelve las últimas 100 líneas; usar 'filter'/'source' para acotar sin gastar contexto."
    )]
    async fn flutter_get_logs(
        &self,
        Parameters(params): Parameters<GetLogsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = LogFilter {
            contains: params.filter,
            source: params.source,
            limit: params.limit.unwrap_or(100) as usize,
        };

        match self.app_service.get_logs(filter).await {
            Ok(entries) => {
                if entries.is_empty() {
                    return Ok(CallToolResult::success(vec![ContentBlock::text(
                        "No hay logs que coincidan con el filtro (o aún no se registró ninguno desde la conexión)",
                    )]));
                }
                let text = entries
                    .iter()
                    .map(|e| {
                        format!(
                            "[{}][{}] {}",
                            format_timestamp_ms(e.timestamp_ms),
                            e.source_label(),
                            e.message
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error obteniendo logs: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Leer errores de framework (red screens: build/layout/paint) que la app haya impreso por stdout/stderr — vía debugPrint por defecto, o un FlutterError.onError personalizado que también imprima. LIMITACIÓN VALIDADA contra una app Flutter real: esto NO detecta excepciones Dart/async genéricas no capturadas, porque el engine las reporta directo a stderr nativo (fuera del sink dart:io que este VM Service observa) y, en la misma prueba, precise=true tampoco las capturó. Útil para errores de widgets, no como red de seguridad general de crashes."
    )]
    async fn flutter_get_errors(
        &self,
        Parameters(params): Parameters<GetErrorsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let precise = params.precise.unwrap_or(false);
        let limit = params.limit.unwrap_or(20) as usize;

        match self.app_service.get_errors(precise).await {
            Ok(mut errors) => {
                if errors.is_empty() {
                    return Ok(CallToolResult::success(vec![ContentBlock::text(
                        "No se detectaron excepciones no manejadas desde la conexión",
                    )]));
                }
                if errors.len() > limit {
                    let split_at = errors.len() - limit;
                    errors = errors.split_off(split_at);
                }

                const MAX_STACK_LINES: usize = 10;
                let text = errors
                    .iter()
                    .map(|err| {
                        let occurrences = if err.occurrences > 1 {
                            format!(" (x{})", err.occurrences)
                        } else {
                            String::new()
                        };
                        let mut block = format!(
                            "[{}]{} {}",
                            format_timestamp_ms(err.timestamp_ms),
                            occurrences,
                            err.message
                        );
                        if let Some(stack) = &err.stack_trace {
                            let lines: Vec<&str> = stack.lines().collect();
                            let shown = lines
                                .iter()
                                .take(MAX_STACK_LINES)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("\n");
                            block.push('\n');
                            block.push_str(&shown);
                            if lines.len() > MAX_STACK_LINES {
                                block.push_str(&format!(
                                    "\n  ... ({} líneas más de stack, no incluidas)",
                                    lines.len() - MAX_STACK_LINES
                                ));
                            }
                        }
                        block
                    })
                    .collect::<Vec<_>>()
                    .join("\n---\n");
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error obteniendo excepciones: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Obtener un reporte de rendimiento (jank, tiempos de build/raster por frame) derivado del stream Timeline acumulado desde la conexión. Devuelve un resumen agregado por defecto; usar include_frames=true para el detalle frame a frame."
    )]
    async fn flutter_get_performance(
        &self,
        Parameters(params): Parameters<GetPerformanceParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.get_performance(params.window_ms).await {
            Ok(report) => {
                if report.frame_count == 0 {
                    return Ok(CallToolResult::success(vec![ContentBlock::text(
                        "No se registraron frames de Timeline en la ventana solicitada (puede que la app no haya generado actividad de UI desde la conexión, o que la ventana sea muy corta)",
                    )]));
                }
                let jank_pct = (report.janky_count as f64 / report.frame_count as f64) * 100.0;
                let mut text = format!(
                    "Frames analizados: {}\nFrames con jank: {} ({:.1}%)\nBuild promedio: {}µs\nRaster promedio: {}µs",
                    report.frame_count,
                    report.janky_count,
                    jank_pct,
                    report.avg_build_us,
                    report.avg_raster_us
                );
                if let Some(worst) = &report.worst_frame {
                    text.push_str(&format!(
                        "\nPeor frame: #{} (build={}µs, raster={}µs, jank={})",
                        worst.frame_number,
                        worst.build_duration_us,
                        worst.raster_duration_us,
                        worst.is_janky
                    ));
                }
                if params.include_frames.unwrap_or(false) {
                    text.push_str("\n\nDetalle por frame:\n");
                    text.push_str(
                        &report
                            .frames
                            .iter()
                            .map(|f| {
                                format!(
                                    "#{}: build={}µs raster={}µs jank={}",
                                    f.frame_number,
                                    f.build_duration_us,
                                    f.raster_duration_us,
                                    f.is_janky
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error obteniendo reporte de rendimiento: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Ejecutar un comando arbitrario de la extensión ext.flutter.driver por su nombre (passthrough directo, sin necesitar una tool dedicada). Útil para comandos del SDK no cubiertos todavía por otra tool ('set_frame_sync', 'set_text_entry_emulation', 'send_text_input_action', etc.) o una extensión de Flutter Driver personalizada de la app. El servidor agrega automáticamente 'command'/'isolateId' y aplica la misma configuración lazy de frame-sync/text-entry-emulation y validación de isError que el resto de las tools de gestos."
    )]
    async fn flutter_driver_raw(
        &self,
        Parameters(params): Parameters<DriverRawParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let command = params.command.clone();
        match self
            .app_service
            .driver_raw(params.command, params.params)
            .await
        {
            Ok(result) => {
                let json_repr = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "Error al serializar respuesta".into());
                Ok(CallToolResult::success(vec![ContentBlock::text(json_repr)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ejecutando comando '{command}' de Flutter Driver: {e}"
            ))])),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FlutterMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("flutter-mcp", "0.2.0"))
    }
}
