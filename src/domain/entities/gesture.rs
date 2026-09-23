use super::finder::Finder;
use serde::{Deserialize, Serialize};

/// Acciones y gestos que se pueden ejecutar sobre un widget o pantalla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Gesture {
    Tap {
        finder: Finder,
        /// Override explícito del timeout (ms) del comando de Flutter Driver. `None` deja que
        /// `precheck_finder` decida entre el fast-fail y el default; `Some(t)` lo salta por
        /// completo y usa `t` tal cual. No tiene relación con `duration_ms` de `Scroll` (esa es
        /// la duración del gesto, no un timeout de espera).
        timeout_ms: Option<u64>,
    },
    EnterText {
        finder: Finder,
        text: String,
        timeout_ms: Option<u64>,
    },
    ClearText {
        finder: Finder,
        timeout_ms: Option<u64>,
    },
    Scroll {
        finder: Finder,
        dx: f64,
        dy: f64,
        duration_ms: u64,
        frequency: u32,
        timeout_ms: Option<u64>,
    },
    ScrollIntoView {
        finder: Finder,
        alignment: f64,
        timeout_ms: Option<u64>,
    },
    ScrollUntilVisible {
        scrollable: Option<Finder>,
        target: Finder,
        delta: f64,
        max_scrolls: u32,
    },
}
