use crate::application::error::{ApplicationError, Result};
use crate::application::ports::inbound::FlutterAppService;
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{Finder, Gesture, WidgetNode};
use crate::domain::services::TreePruner;
use async_trait::async_trait;
use std::sync::Arc;

pub struct FlutterServiceImpl {
    vm_port: Arc<dyn FlutterVmPort>,
}

impl FlutterServiceImpl {
    pub fn new(vm_port: Arc<dyn FlutterVmPort>) -> Self {
        Self { vm_port }
    }
}

#[async_trait]
impl FlutterAppService for FlutterServiceImpl {
    async fn connect(&self, uri: &str) -> Result<()> {
        self.vm_port.connect(uri).await
    }

    async fn disconnect(&self) -> Result<()> {
        self.vm_port.disconnect().await
    }

    async fn get_pruned_snapshot(&self) -> Result<WidgetNode> {
        let raw_tree = self.vm_port.get_diagnostics_tree(50).await?;
        TreePruner::prune_diagnostics_tree(&raw_tree).ok_or_else(|| {
            ApplicationError::Internal("No se pudo extraer ningún widget visible del árbol".into())
        })
    }

    async fn tap(&self, finder: Finder) -> Result<()> {
        let gesture = Gesture::Tap { finder };
        self.vm_port.dispatch_gesture(&gesture).await
    }

    async fn enter_text(&self, finder: Finder, text: String) -> Result<()> {
        let gesture = Gesture::EnterText { finder, text };
        self.vm_port.dispatch_gesture(&gesture).await
    }

    async fn hot_reload(&self) -> Result<()> {
        self.vm_port.trigger_hot_reload().await
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        self.vm_port.capture_screenshot().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::outbound::MockFlutterVmPort;
    use serde_json::json;

    #[tokio::test]
    async fn test_get_pruned_snapshot_success() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm
            .expect_get_diagnostics_tree()
            .with(mockall::predicate::eq(50))
            .times(1)
            .returning(|_| {
                Ok(json!({
                    "description": "Scaffold",
                    "children": [
                        {
                            "description": "ElevatedButton",
                            "properties": [{"name": "key", "description": "[<'login_btn'>]"}],
                            "children": []
                        }
                    ]
                }))
            });

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        let snapshot = service
            .get_pruned_snapshot()
            .await
            .expect("Snapshot should succeed");

        assert_eq!(snapshot.widget_type, "Scaffold");
        assert_eq!(snapshot.children.len(), 1);
        assert_eq!(snapshot.children[0].widget_type, "ElevatedButton");
        assert_eq!(snapshot.children[0].key.as_deref(), Some("[<'login_btn'>]"));
    }

    #[tokio::test]
    async fn test_tap_dispatches_gesture_to_port() {
        let mut mock_vm = MockFlutterVmPort::new();
        let expected_finder = Finder::by_key("submit_btn");
        let expected_gesture = Gesture::Tap {
            finder: expected_finder.clone(),
        };

        mock_vm
            .expect_dispatch_gesture()
            .with(mockall::predicate::eq(expected_gesture))
            .times(1)
            .returning(|_| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        service
            .tap(expected_finder)
            .await
            .expect("Tap should succeed");
    }
}
