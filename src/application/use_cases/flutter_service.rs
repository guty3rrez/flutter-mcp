use crate::application::error::{ApplicationError, Result};
use crate::application::ports::inbound::FlutterAppService;
use crate::application::ports::outbound::FlutterVmPort;
use crate::domain::entities::{
    Finder, FlutterError, Gesture, LogEntry, LogFilter, PerformanceReport, WidgetNode,
};
use crate::domain::services::{DEFAULT_FRAME_BUDGET_US, TimelineAnalyzer, TreePruner};
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

    async fn get_logs(&self, filter: LogFilter) -> Result<Vec<LogEntry>> {
        let mut entries = self.vm_port.get_logs().await?;

        if let Some(source) = &filter.source {
            entries.retain(|e| e.source_label().eq_ignore_ascii_case(source));
        }
        if let Some(needle) = &filter.contains {
            let needle_lower = needle.to_lowercase();
            entries.retain(|e| e.message.to_lowercase().contains(&needle_lower));
        }

        if filter.limit > 0 && entries.len() > filter.limit {
            let split_at = entries.len() - filter.limit;
            entries = entries.split_off(split_at);
        }

        Ok(entries)
    }

    async fn get_errors(&self, precise: bool) -> Result<Vec<FlutterError>> {
        if precise {
            self.vm_port.enable_precise_error_mode(true).await?;
        }
        self.vm_port.get_errors().await
    }

    async fn get_performance(&self, window_ms: Option<u64>) -> Result<PerformanceReport> {
        let mut events = self.vm_port.get_raw_timeline_events().await?;

        if let Some(window_ms) = window_ms {
            let max_ts = events
                .iter()
                .filter_map(|e| e.get("ts").and_then(|t| t.as_i64()))
                .max();
            if let Some(max_ts) = max_ts {
                let cutoff = max_ts - (window_ms as i64 * 1000);
                events.retain(|e| {
                    e.get("ts")
                        .and_then(|t| t.as_i64())
                        .map(|ts| ts >= cutoff)
                        .unwrap_or(false)
                });
            }
        }

        let frames = TimelineAnalyzer::compute_frame_timings(&events, DEFAULT_FRAME_BUDGET_US);
        Ok(TimelineAnalyzer::summarize(&frames))
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

    #[tokio::test]
    async fn test_get_logs_applies_filter_and_limit() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_get_logs().times(1).returning(|| {
            Ok(vec![
                LogEntry {
                    timestamp_ms: 1,
                    source: crate::domain::entities::LogSource::Stdout,
                    message: "primer log".into(),
                },
                LogEntry {
                    timestamp_ms: 2,
                    source: crate::domain::entities::LogSource::Stderr,
                    message: "error de red".into(),
                },
                LogEntry {
                    timestamp_ms: 3,
                    source: crate::domain::entities::LogSource::Stdout,
                    message: "segundo log de red".into(),
                },
            ])
        });

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        let filter = LogFilter {
            contains: Some("red".into()),
            source: Some("stdout".into()),
            limit: 10,
        };
        let logs = service.get_logs(filter).await.expect("Debe filtrar logs");

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "segundo log de red");
    }

    #[tokio::test]
    async fn test_get_logs_respects_limit_keeping_most_recent() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_get_logs().times(1).returning(|| {
            Ok((0..5)
                .map(|i| LogEntry {
                    timestamp_ms: i,
                    source: crate::domain::entities::LogSource::Stdout,
                    message: format!("log {i}"),
                })
                .collect())
        });

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        let logs = service
            .get_logs(LogFilter {
                contains: None,
                source: None,
                limit: 2,
            })
            .await
            .expect("Debe limitar logs");

        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "log 3");
        assert_eq!(logs[1].message, "log 4");
    }

    #[tokio::test]
    async fn test_get_errors_enables_precise_mode_when_requested() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm
            .expect_enable_precise_error_mode()
            .with(mockall::predicate::eq(true))
            .times(1)
            .returning(|_| Ok(()));
        mock_vm
            .expect_get_errors()
            .times(1)
            .returning(|| Ok(vec![]));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        service
            .get_errors(true)
            .await
            .expect("Debe activar modo preciso y leer errores");
    }

    #[tokio::test]
    async fn test_get_errors_skips_precise_mode_by_default() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_enable_precise_error_mode().times(0);
        mock_vm
            .expect_get_errors()
            .times(1)
            .returning(|| Ok(vec![]));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        service
            .get_errors(false)
            .await
            .expect("Debe leer errores sin activar modo preciso");
    }

    #[tokio::test]
    async fn test_get_performance_summarizes_raw_timeline_events() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm
            .expect_get_raw_timeline_events()
            .times(1)
            .returning(|| {
                Ok(vec![
                    json!({ "name": "Animator::BeginFrame", "ph": "B", "ts": 0 }),
                    json!({ "name": "Animator::BeginFrame", "ph": "E", "ts": 8_000 }),
                    json!({ "name": "Rasterizer::DrawToSurfaces", "ph": "B", "ts": 8_000 }),
                    json!({ "name": "Rasterizer::DrawToSurfaces", "ph": "E", "ts": 14_000 }),
                ])
            });

        let service = FlutterServiceImpl::new(Arc::new(mock_vm));
        let report = service
            .get_performance(None)
            .await
            .expect("Debe generar reporte de performance");

        assert_eq!(report.frame_count, 1);
        assert_eq!(report.janky_count, 0);
        assert_eq!(report.avg_build_us, 8_000);
        assert_eq!(report.avg_raster_us, 6_000);
    }
}
