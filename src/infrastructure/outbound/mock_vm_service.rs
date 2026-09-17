use crate::application::error::{ApplicationError, Result};
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{Finder, Gesture};
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
        }
    }

    pub async fn set_mock_tree(&self, tree: Value) {
        let mut t = self.mock_tree.lock().await;
        *t = tree;
    }

    pub async fn get_dispatched_gestures(&self) -> Vec<Gesture> {
        self.dispatched_gestures.lock().await.clone()
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
}
