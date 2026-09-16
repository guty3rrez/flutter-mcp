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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConnectParams {
    #[schemars(
        description = "URI del Dart VM Service de la app Flutter (ej: ws://127.0.0.1:45678/ws)"
    )]
    pub uri: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TapParams {
    #[schemars(description = "Criterio de búsqueda: 'key', 'text', 'tooltip', o 'type'")]
    pub by: String,
    #[schemars(description = "Valor a buscar (nombre de la key, texto exacto, tooltip, etc.)")]
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EnterTextParams {
    #[schemars(description = "Criterio de búsqueda: 'key', 'text', 'tooltip', o 'type'")]
    pub by: String,
    #[schemars(description = "Valor a buscar")]
    pub value: String,
    #[schemars(description = "Texto a ingresar en el campo")]
    pub text: String,
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
        description = "Hacer tap (clic) sobre un widget localizado por Key, texto, tooltip o tipo"
    )]
    async fn flutter_tap(
        &self,
        Parameters(params): Parameters<TapParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let finder = match params.by.as_str() {
            "key" => Finder::by_key(params.value),
            "text" => Finder::by_text(params.value, true),
            "tooltip" => Finder::Tooltip(params.value),
            "type" => Finder::by_type(params.value),
            other => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Criterio de búsqueda desconocido '{other}'. Use 'key', 'text', 'tooltip' o 'type'"
                ))]));
            }
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
        let finder = match params.by.as_str() {
            "key" => Finder::by_key(params.value),
            "text" => Finder::by_text(params.value, true),
            "tooltip" => Finder::Tooltip(params.value),
            "type" => Finder::by_type(params.value),
            other => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Criterio de búsqueda desconocido '{other}'. Use 'key', 'text', 'tooltip' o 'type'"
                ))]));
            }
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
        description = "Tomar una captura de pantalla (screenshot) de la interfaz de la app Flutter"
    )]
    async fn flutter_screenshot(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.app_service.take_screenshot().await {
            Ok(bytes) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "Captura de pantalla tomada con éxito ({} bytes)",
                bytes.len()
            ))])),
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
            .with_server_info(Implementation::new("flutter-native-mcp", "0.1.0"))
    }
}
