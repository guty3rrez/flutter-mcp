use super::finder::Finder;
use serde::{Deserialize, Serialize};

/// Acciones y gestos que se pueden ejecutar sobre un widget o pantalla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Gesture {
    Tap {
        finder: Finder,
    },
    EnterText {
        finder: Finder,
        text: String,
    },
    ClearText {
        finder: Finder,
    },
    Scroll {
        finder: Finder,
        dx: f64,
        dy: f64,
        duration_ms: u64,
    },
    ScrollUntilVisible {
        scrollable: Option<Finder>,
        target: Finder,
        delta: f64,
        max_scrolls: u32,
    },
}
