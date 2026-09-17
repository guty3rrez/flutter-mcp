use crate::application::ports::inbound::FlutterAppService;
use crate::domain::entities::Finder;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

        match self.app_service.tap(finder).await {
            Ok(_) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Tap ejecutado exitosamente",
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error ejecutando tap: {e}"
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

        match self.app_service.enter_text(finder, params.text).await {
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
            .scroll(finder, params.dx, params.dy, duration_ms, frequency)
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

        match self.app_service.scroll_into_view(finder, alignment).await {
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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FlutterMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("flutter-native-mcp", "0.2.0"))
    }
}
