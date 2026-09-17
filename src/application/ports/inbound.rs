use crate::application::error::Result;
use crate::domain::entities::{Finder, WidgetNode};
use async_trait::async_trait;

#[async_trait]
pub trait FlutterAppService: Send + Sync {
    async fn connect(&self, uri: &str) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn get_pruned_snapshot(&self) -> Result<WidgetNode>;
    async fn tap(&self, finder: Finder) -> Result<()>;
    async fn enter_text(&self, finder: Finder, text: String) -> Result<()>;
    async fn get_text(&self, finder: Finder) -> Result<String>;
    async fn scroll(
        &self,
        finder: Finder,
        dx: f64,
        dy: f64,
        duration_ms: u64,
        frequency: u32,
    ) -> Result<()>;
    async fn scroll_into_view(&self, finder: Finder, alignment: f64) -> Result<()>;
    async fn wait_for(&self, finder: Finder, timeout_ms: u64) -> Result<()>;
    async fn wait_for_absent(&self, finder: Finder, timeout_ms: u64) -> Result<()>;
    async fn hot_reload(&self) -> Result<()>;
    async fn hot_restart(&self) -> Result<()>;
    async fn take_screenshot(&self) -> Result<Vec<u8>>;
}
