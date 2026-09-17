use serde::{Deserialize, Serialize};

/// Coordenadas y dimensiones de un widget en pantalla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RectBounds {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl RectBounds {
    pub fn new(left: f64, top: f64, width: f64, height: f64) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    pub fn center(&self) -> (f64, f64) {
        (self.left + self.width / 2.0, self.top + self.height / 2.0)
    }
}

/// Nodo simplificado del árbol de UI de Flutter, optimizado para consumo de LLMs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetNode {
    pub widget_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantics_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<RectBounds>,
    pub is_interactive: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<WidgetNode>,
}

impl WidgetNode {
    pub fn new(widget_type: impl Into<String>, is_interactive: bool) -> Self {
        Self {
            widget_type: widget_type.into(),
            key: None,
            text: None,
            tooltip: None,
            semantics_label: None,
            bounds: None,
            is_interactive,
            children: Vec::new(),
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_semantics_label(mut self, label: impl Into<String>) -> Self {
        self.semantics_label = Some(label.into());
        self
    }

    pub fn with_bounds(mut self, bounds: RectBounds) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn add_child(&mut self, child: WidgetNode) {
        self.children.push(child);
    }
}
