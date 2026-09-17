pub mod application;
pub mod domain;
pub mod infrastructure;

// Explicit re-exports to avoid ambiguous globs
pub use application::error::{ApplicationError, Result};
pub use application::ports::inbound::FlutterAppService;
pub use application::ports::outbound::FlutterVmPort;
pub use application::use_cases::FlutterServiceImpl;

pub use domain::entities::{
    ErrorSource, Finder, FlutterError, FrameTiming, Gesture, LogEntry, LogFilter, LogSource,
    PerformanceReport, RectBounds, WidgetNode,
};
pub use domain::services::{ErrorDetector, LogParser, TimelineAnalyzer, TreePruner};

pub use infrastructure::inbound::FlutterMcpServer;
pub use infrastructure::outbound::{MockVmServiceAdapter, WebSocketVmServiceAdapter};
