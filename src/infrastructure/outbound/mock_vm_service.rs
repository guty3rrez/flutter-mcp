use crate::application::error::{ApplicationError, Result};
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{Finder, FlutterError, Gesture, LogEntry};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

/// Adaptador simulado para pruebas de integración y desarrollo local sin app Flutter activa
pub struct MockVmServiceAdapter {
    connected: AtomicBool,
    dispatched_gestures: Arc<Mutex<Vec<Gesture>>>,
    mock_tree: Arc<Mutex<Value>>,
    mock_logs: Arc<Mutex<Vec<LogEntry>>>,
    mock_errors: Arc<Mutex<Vec<FlutterError>>>,
    mock_timeline_events: Arc<Mutex<Vec<Value>>>,
    precise_error_mode_enabled: AtomicBool,
}

impl MockVmServiceAdapter {
    pub fn new() -> Self {
        let default_tree = json!({
            "description": "Scaffold",
            "children": [
                {
                    "description": "AppBar",
                    "children": [
                        {
                            "description": "Text",
                            "properties": [{"name": "data", "description": "\"Mi App Flutter\""}]
                        }
                    ]
                },
                {
                    "description": "Padding",
                    "children": [
                        {
                            "description": "ElevatedButton",
                            "properties": [
                                {"name": "key", "description": "[<'btn_counter'>]"}
                            ],
                            "children": [
                                {
                                    "description": "Text",
                                    "properties": [{"name": "data", "description": "\"Incrementar\""}]
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        Self {
            connected: AtomicBool::new(false),
            dispatched_gestures: Arc::new(Mutex::new(Vec::new())),
            mock_tree: Arc::new(Mutex::new(default_tree)),
            mock_logs: Arc::new(Mutex::new(Vec::new())),
            mock_errors: Arc::new(Mutex::new(Vec::new())),
            mock_timeline_events: Arc::new(Mutex::new(Vec::new())),
            precise_error_mode_enabled: AtomicBool::new(false),
        }
    }

    pub async fn set_mock_tree(&self, tree: Value) {
        let mut t = self.mock_tree.lock().await;
        *t = tree;
    }

    pub async fn get_dispatched_gestures(&self) -> Vec<Gesture> {
        self.dispatched_gestures.lock().await.clone()
    }

    /// Inyecta una línea de log sintética, para probar `flutter_get_logs` sin una app real.
    pub async fn push_mock_log(&self, entry: LogEntry) {
        self.mock_logs.lock().await.push(entry);
    }

    /// Inyecta un error sintético, para probar `flutter_get_errors` sin una app real.
    pub async fn push_mock_error(&self, error: FlutterError) {
        self.mock_errors.lock().await.push(error);
    }

    /// Inyecta un evento crudo de timeline sintético, para probar `flutter_get_performance`.
    pub async fn push_mock_timeline_event(&self, event: Value) {
        self.mock_timeline_events.lock().await.push(event);
    }

    pub fn is_precise_error_mode_enabled(&self) -> bool {
        self.precise_error_mode_enabled.load(Ordering::SeqCst)
    }
}

impl Default for MockVmServiceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FlutterVmPort for MockVmServiceAdapter {
    async fn connect(&self, _vm_service_uri: &str) -> Result<()> {
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn get_diagnostics_tree(&self, _subtree_depth: u32) -> Result<Value> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(self.mock_tree.lock().await.clone())
    }

    async fn execute_driver_command(&self, command: &str, _params: Value) -> Result<Value> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(json!({"status": "ok", "command": command}))
    }

    async fn dispatch_gesture(&self, gesture: &Gesture) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        self.dispatched_gestures.lock().await.push(gesture.clone());
        Ok(())
    }

    async fn get_text(&self, finder: &Finder) -> Result<String> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        match finder {
            Finder::Text { text, .. } => Ok(text.clone()),
            Finder::Key(k) => Ok(format!("Text for {k}")),
            _ => Ok("Mock Widget Text".into()),
        }
    }

    async fn wait_for(&self, _finder: &Finder, _timeout_ms: u64) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(())
    }

    async fn wait_for_absent(&self, _finder: &Finder, _timeout_ms: u64) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(())
    }

    async fn trigger_hot_reload(&self) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(())
    }

    async fn trigger_hot_restart(&self) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(())
    }

    async fn capture_screenshot(&self) -> Result<Vec<u8>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        // Retorna un PNG mínimo válido de 1x1 píxel
        Ok(vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ])
    }

    async fn get_logs(&self) -> Result<Vec<LogEntry>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(self.mock_logs.lock().await.clone())
    }

    async fn get_errors(&self) -> Result<Vec<FlutterError>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(self.mock_errors.lock().await.clone())
    }

    async fn enable_precise_error_mode(&self, enabled: bool) -> Result<()> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        self.precise_error_mode_enabled
            .store(enabled, Ordering::SeqCst);
        Ok(())
    }

    async fn get_raw_timeline_events(&self) -> Result<Vec<Value>> {
        if !self.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }
        Ok(self.mock_timeline_events.lock().await.clone())
    }
}
