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

    async fn get_text(&self, finder: Finder) -> Result<String> {
        self.vm_port.get_text(&finder).await
    }

    async fn scroll(
        &self,
        finder: Finder,
        dx: f64,
        dy: f64,
        duration_ms: u64,
        frequency: u32,
    ) -> Result<()> {
        let gesture = Gesture::Scroll {
            finder,
            dx,
            dy,
            duration_ms,
            frequency,
        };
        self.vm_port.dispatch_gesture(&gesture).await
    }

    async fn scroll_into_view(&self, finder: Finder, alignment: f64) -> Result<()> {
        let gesture = Gesture::ScrollIntoView { finder, alignment };
        self.vm_port.dispatch_gesture(&gesture).await
    }

    async fn wait_for(&self, finder: Finder, timeout_ms: u64) -> Result<()> {
        self.vm_port.wait_for(&finder, timeout_ms).await
    }

    async fn wait_for_absent(&self, finder: Finder, timeout_ms: u64) -> Result<()> {
        self.vm_port.wait_for_absent(&finder, timeout_ms).await
    }

    async fn hot_reload(&self) -> Result<()> {
        self.vm_port.trigger_hot_reload().await
    }

    async fn hot_restart(&self) -> Result<()> {
        self.vm_port.trigger_hot_restart().await
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

    #[tokio::test]
    async fn test_get_text_delegates_to_port() {
        let mut mock_vm = MockFlutterVmPort::new();
        let finder = Finder::by_key("title");

        mock_vm
            .expect_get_text()
            .with(mockall::predicate::eq(finder.clone()))
            .times(1)
            .returning(|_| Ok("Auralis Music".into()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        let text = service
            .get_text(finder)
            .await
            .expect("GetText should succeed");
        assert_eq!(text, "Auralis Music");
    }

    #[tokio::test]
    async fn test_scroll_dispatches_scroll_gesture() {
        let mut mock_vm = MockFlutterVmPort::new();
        let finder = Finder::by_type("ListView");
        let expected_gesture = Gesture::Scroll {
            finder: finder.clone(),
            dx: 0.0,
            dy: -200.0,
            duration_ms: 300,
            frequency: 60,
        };

        mock_vm
            .expect_dispatch_gesture()
            .with(mockall::predicate::eq(expected_gesture))
            .times(1)
            .returning(|_| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        service
            .scroll(finder, 0.0, -200.0, 300, 60)
            .await
            .expect("Scroll should succeed");
    }

    #[tokio::test]
    async fn test_wait_for_delegates_to_port() {
        let mut mock_vm = MockFlutterVmPort::new();
        let finder = Finder::by_key("loaded_view");

        mock_vm
            .expect_wait_for()
            .with(
                mockall::predicate::eq(finder.clone()),
                mockall::predicate::eq(3000),
            )
            .times(1)
            .returning(|_, _| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        service
            .wait_for(finder, 3000)
            .await
            .expect("WaitFor should succeed");
    }
}
