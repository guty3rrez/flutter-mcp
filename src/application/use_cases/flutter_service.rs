use crate::application::error::{ApplicationError, Result};
use crate::application::ports::inbound::FlutterAppService;
use crate::application::ports::outbound::{FlutterVmPort, ProjectFilesPort};
use crate::domain::entities::{
    Finder, FlutterError, Gesture, LogEntry, LogFilter, PerformanceReport, StartControlOutcome,
    WidgetNode,
};
use crate::domain::services::{
    DEFAULT_FRAME_BUDGET_US, DriverInjector, PubspecEditor, TimelineAnalyzer, TreePruner,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct FlutterServiceImpl {
    vm_port: Arc<dyn FlutterVmPort>,
    files_port: Arc<dyn ProjectFilesPort>,
}

impl FlutterServiceImpl {
    pub fn new(vm_port: Arc<dyn FlutterVmPort>, files_port: Arc<dyn ProjectFilesPort>) -> Self {
        Self {
            vm_port,
            files_port,
        }
    }

    fn join_path(root: &str, relative: &str) -> String {
        format!(
            "{}/{}",
            root.trim_end_matches('/'),
            relative.trim_start_matches('/')
        )
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

    async fn start_control(
        &self,
        project_root: String,
        entrypoint: String,
        revert_after_restart: bool,
    ) -> Result<StartControlOutcome> {
        if !self.vm_port.is_connected().await {
            return Err(ApplicationError::NotConnected);
        }

        let pubspec_path = Self::join_path(&project_root, "pubspec.yaml");
        let entrypoint_path = Self::join_path(&project_root, &entrypoint);

        let pubspec_source = self
            .files_port
            .read_to_string(&pubspec_path)
            .await
            .map_err(|_| {
                ApplicationError::FileSystemError(format!(
                    "No se encontró pubspec.yaml en '{pubspec_path}' (project_root='{project_root}'). \
El servidor MCP no corre necesariamente en el directorio de la app Flutter conectada -- pasá \
'project_root' con la ruta absoluta de esa app (la que contiene pubspec.yaml), no el default."
                ))
            })?;
        if !PubspecEditor::declares_flutter_driver(&pubspec_source) {
            let patched = PubspecEditor::add_flutter_driver_dev_dependency(&pubspec_source);
            self.files_port
                .write_string(&pubspec_path, &patched)
                .await?;

            // No se puede seguir: la dependencia recién agregada no está resuelta en
            // pubspec.lock todavía, así que ni evaluarla ni un Hot Restart la haría disponible.
            return Ok(StartControlOutcome {
                entrypoint_path,
                already_enabled: false,
                pubspec_updated: true,
                hot_restart_triggered: false,
                reverted: false,
            });
        }

        let original_source = self.files_port.read_to_string(&entrypoint_path).await?;
        let already_enabled = DriverInjector::already_enabled(&original_source);

        if !already_enabled {
            let patched = DriverInjector::inject(&original_source)
                .ok_or_else(|| ApplicationError::MainNotFound(entrypoint_path.clone()))?;
            self.files_port
                .write_string(&entrypoint_path, &patched)
                .await?;
        }

        self.vm_port.trigger_hot_restart().await?;

        let mut reverted = false;
        if revert_after_restart && !already_enabled {
            self.files_port
                .write_string(&entrypoint_path, &original_source)
                .await?;
            reverted = true;
        }

        Ok(StartControlOutcome {
            entrypoint_path,
            already_enabled,
            pubspec_updated: false,
            hot_restart_triggered: true,
            reverted,
        })
    }

    async fn driver_raw(
        &self,
        command: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.vm_port.execute_driver_command(&command, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::outbound::{MockFlutterVmPort, MockProjectFilesPort};
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
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

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
        let report = service
            .get_performance(None)
            .await
            .expect("Debe generar reporte de performance");

        assert_eq!(report.frame_count, 1);
        assert_eq!(report.janky_count, 0);
        assert_eq!(report.avg_build_us, 8_000);
        assert_eq!(report.avg_raster_us, 6_000);
    }

    #[tokio::test]
    async fn test_start_control_fails_when_not_connected() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| false);

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
        let result = service
            .start_control(".".into(), "lib/main.dart".into(), false)
            .await;

        assert!(matches!(result, Err(ApplicationError::NotConnected)));
    }

    #[tokio::test]
    async fn test_start_control_adds_missing_pubspec_dependency_and_stops() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| true);

        let mut mock_files = MockProjectFilesPort::new();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./pubspec.yaml"))
            .times(1)
            .returning(|_| Ok("name: my_app\n".to_string()));
        mock_files
            .expect_write_string()
            .withf(|path, content| {
                path == "./pubspec.yaml" && content.contains("flutter_driver:\n    sdk: flutter")
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(mock_files));
        let outcome = service
            .start_control(".".into(), "lib/main.dart".into(), false)
            .await
            .expect("debe agregar la dependencia y detenerse");

        assert!(outcome.pubspec_updated);
        assert!(!outcome.hot_restart_triggered);
        assert!(!outcome.already_enabled);
    }

    #[tokio::test]
    async fn test_start_control_injects_and_triggers_hot_restart() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| true);
        mock_vm
            .expect_trigger_hot_restart()
            .times(1)
            .returning(|| Ok(()));

        let mut mock_files = MockProjectFilesPort::new();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./pubspec.yaml"))
            .times(1)
            .returning(|_| {
                Ok("dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n".to_string())
            });
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./lib/main.dart"))
            .times(1)
            .returning(|_| Ok("void main() {\n  runApp(const MyApp());\n}\n".to_string()));
        mock_files
            .expect_write_string()
            .withf(|path, content| {
                path == "./lib/main.dart" && content.contains("enableFlutterDriverExtension();")
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(mock_files));
        let outcome = service
            .start_control(".".into(), "lib/main.dart".into(), false)
            .await
            .expect("debe inyectar y reiniciar");

        assert!(!outcome.already_enabled);
        assert!(!outcome.pubspec_updated);
        assert!(outcome.hot_restart_triggered);
        assert!(!outcome.reverted);
    }

    #[tokio::test]
    async fn test_start_control_skips_injection_when_already_enabled() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| true);
        mock_vm
            .expect_trigger_hot_restart()
            .times(1)
            .returning(|| Ok(()));

        let mut mock_files = MockProjectFilesPort::new();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./pubspec.yaml"))
            .times(1)
            .returning(|_| {
                Ok("dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n".to_string())
            });
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./lib/main.dart"))
            .times(1)
            .returning(|_| {
                Ok(
                    "void main() {\n  enableFlutterDriverExtension();\n  runApp(const MyApp());\n}\n"
                        .to_string(),
                )
            });
        mock_files.expect_write_string().times(0);

        let service = FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(mock_files));
        let outcome = service
            .start_control(".".into(), "lib/main.dart".into(), false)
            .await
            .expect("debe detectar que ya estaba habilitado");

        assert!(outcome.already_enabled);
        assert!(outcome.hot_restart_triggered);
    }

    #[tokio::test]
    async fn test_start_control_reverts_entrypoint_when_requested() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| true);
        mock_vm
            .expect_trigger_hot_restart()
            .times(1)
            .returning(|| Ok(()));

        let original = "void main() {\n  runApp(const MyApp());\n}\n".to_string();
        let mut mock_files = MockProjectFilesPort::new();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./pubspec.yaml"))
            .times(1)
            .returning(|_| {
                Ok("dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n".to_string())
            });
        let original_for_read = original.clone();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./lib/main.dart"))
            .times(1)
            .returning(move |_| Ok(original_for_read.clone()));
        mock_files
            .expect_write_string()
            .withf(|path, content| {
                path == "./lib/main.dart" && content.contains("enableFlutterDriverExtension")
            })
            .times(1)
            .returning(|_, _| Ok(()));
        let original_for_revert = original.clone();
        mock_files
            .expect_write_string()
            .withf(move |path, content| path == "./lib/main.dart" && content == original_for_revert)
            .times(1)
            .returning(|_, _| Ok(()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(mock_files));
        let outcome = service
            .start_control(".".into(), "lib/main.dart".into(), true)
            .await
            .expect("debe revertir tras el hot restart");

        assert!(outcome.reverted);
    }

    #[tokio::test]
    async fn test_start_control_fails_when_main_not_found() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm.expect_is_connected().times(1).returning(|| true);

        let mut mock_files = MockProjectFilesPort::new();
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./pubspec.yaml"))
            .times(1)
            .returning(|_| {
                Ok("dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n".to_string())
            });
        mock_files
            .expect_read_to_string()
            .with(mockall::predicate::eq("./lib/main.dart"))
            .times(1)
            .returning(|_| Ok("class Foo {}\n".to_string()));

        let service = FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(mock_files));
        let result = service
            .start_control(".".into(), "lib/main.dart".into(), false)
            .await;

        assert!(matches!(result, Err(ApplicationError::MainNotFound(_))));
    }

    #[tokio::test]
    async fn test_driver_raw_delegates_to_execute_driver_command() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm
            .expect_execute_driver_command()
            .withf(|command, params| {
                command == "get_semantics_id"
                    && params == &json!({"finderType":"ByType","type":"Text"})
            })
            .times(1)
            .returning(|_, _| Ok(json!({"isError": false, "response": {"id": 42}})));

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
        let result = service
            .driver_raw(
                "get_semantics_id".into(),
                json!({"finderType":"ByType","type":"Text"}),
            )
            .await
            .expect("debe delegar exitosamente");

        assert_eq!(result["response"]["id"], 42);
    }

    #[tokio::test]
    async fn test_driver_raw_propagates_port_error() {
        let mut mock_vm = MockFlutterVmPort::new();
        mock_vm
            .expect_execute_driver_command()
            .times(1)
            .returning(|_, _| Err(ApplicationError::DriverError("boom".into())));

        let service =
            FlutterServiceImpl::new(Arc::new(mock_vm), Arc::new(MockProjectFilesPort::new()));
        let result = service.driver_raw("x".into(), json!({})).await;
        assert!(matches!(result, Err(ApplicationError::DriverError(_))));
    }
}
