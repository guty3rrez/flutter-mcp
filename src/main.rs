use rmcp::ServiceExt;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use flutter_mcp::{
    FlutterMcpServer, FlutterServiceImpl, FlutterVmPort, LocalFileSystemAdapter,
    MockVmServiceAdapter, WebSocketVmServiceAdapter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Importante para servidores MCP stdio: logs SIEMPRE a stderr
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "flutter_mcp=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Iniciando Flutter MCP Server...");

    // Soporte para modo simulación (flag --mock) o conexión real a WebSocket
    let args: Vec<String> = std::env::args().collect();
    let use_mock = args.iter().any(|arg| arg == "--mock");

    let vm_port: Arc<dyn FlutterVmPort> = if use_mock {
        tracing::info!("Modo MOCK activado para pruebas locales sin app Flutter");
        Arc::new(MockVmServiceAdapter::new())
    } else {
        Arc::new(WebSocketVmServiceAdapter::new())
    };

    let files_port = Arc::new(LocalFileSystemAdapter::new());
    let app_service = Arc::new(FlutterServiceImpl::new(vm_port, files_port));
    let server = FlutterMcpServer::new(app_service);

    tracing::info!("Escuchando en STDIO para comunicación con el agente...");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
