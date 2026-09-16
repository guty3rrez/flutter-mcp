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
    async fn hot_reload(&self) -> Result<()>;
    async fn take_screenshot(&self) -> Result<Vec<u8>>;
}
