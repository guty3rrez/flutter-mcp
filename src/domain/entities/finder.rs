use serde::{Deserialize, Serialize};

/// Criterios de búsqueda polimórficos para localizar widgets en la app Flutter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "by", content = "value", rename_all = "snake_case")]
pub enum Finder {
    /// Búsqueda por ValueKey o Key exacta
    Key(String),
    /// Búsqueda por texto visible
    Text { text: String, exact: bool },
    /// Búsqueda por tooltip o mensaje semántico
    Tooltip(String),
    /// Búsqueda por tipo de widget (ej. 'ElevatedButton')
    Type(String),
    /// Búsqueda por coordenadas exactas (x, y)
    Coordinates { x: f64, y: f64 },
}

impl Finder {
    pub fn by_key(key: impl Into<String>) -> Self {
        Finder::Key(key.into())
    }

    pub fn by_text(text: impl Into<String>, exact: bool) -> Self {
        Finder::Text {
            text: text.into(),
            exact,
        }
    }

    pub fn by_type(widget_type: impl Into<String>) -> Self {
        Finder::Type(widget_type.into())
    }
}
