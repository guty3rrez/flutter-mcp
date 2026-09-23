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
    /// Búsqueda por etiqueta de accesibilidad / semántica
    SemanticsLabel(String),
    /// Búsqueda por coordenadas exactas (x, y)
    Coordinates { x: f64, y: f64 },
    /// Gesto de retroceso estándar (`finderType: "PageBack"` del wire protocol de
    /// `flutter_driver`). **Validado contra el SDK real** (`flutter_driver/lib/src/common/
    /// handler_factory.dart`, `_createPageBackFinder`): matchea únicamente
    /// `Tooltip(message: 'Back')` (string literal en inglés, hardcodeado, sin localizar) o
    /// `CupertinoNavigationBarBackButton` -- NO existe fallback por `BackButtonIcon` ni por tipo
    /// de widget genérico. **Validado contra una app real (ohmycat, locale es-ES)**: no matchea
    /// ni siquiera el `BackButton` *canónico* de Material en una app localizada al español,
    /// porque `MaterialLocalizations.of(context).backButtonTooltip` devuelve "Atrás", no "Back"
    /// -- y mucho menos un botón de retroceso custom (`IconButton(icon: Icon(...))`), el patrón
    /// más común en apps de producción. Útil solo para apps en inglés con el `BackButton`
    /// default, o apps Cupertino. Para cualquier otro caso, usar `Finder::Key` si el botón tiene
    /// una key, que sí funciona de forma confiable.
    PageBack,
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

    pub fn by_tooltip(tooltip: impl Into<String>) -> Self {
        Finder::Tooltip(tooltip.into())
    }

    pub fn by_semantics_label(label: impl Into<String>) -> Self {
        Finder::SemanticsLabel(label.into())
    }

    /// Serializa el Finder en los parámetros requeridos por el protocolo Flutter Driver
    pub fn to_driver_params(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        match self {
            Finder::Key(key_val) => {
                map.insert("finderType".into(), serde_json::json!("ByValueKey"));
                map.insert("keyValueString".into(), serde_json::json!(key_val));
                map.insert("keyValueType".into(), serde_json::json!("String"));
            }
            Finder::Text { text, .. } => {
                map.insert("finderType".into(), serde_json::json!("ByText"));
                map.insert("text".into(), serde_json::json!(text));
            }
            Finder::Tooltip(tip) => {
                map.insert("finderType".into(), serde_json::json!("ByTooltipMessage"));
                map.insert("text".into(), serde_json::json!(tip));
            }
            Finder::Type(t) => {
                map.insert("finderType".into(), serde_json::json!("ByType"));
                map.insert("type".into(), serde_json::json!(t));
            }
            Finder::SemanticsLabel(label) => {
                map.insert("finderType".into(), serde_json::json!("BySemanticsLabel"));
                map.insert("label".into(), serde_json::json!(label));
            }
            Finder::Coordinates { x, y } => {
                map.insert("finderType".into(), serde_json::json!("ByOffset"));
                map.insert("dx".into(), serde_json::json!(x));
                map.insert("dy".into(), serde_json::json!(y));
            }
            Finder::PageBack => {
                map.insert("finderType".into(), serde_json::json!("PageBack"));
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_driver_params_page_back_has_no_extra_fields() {
        let map = Finder::PageBack.to_driver_params();
        assert_eq!(
            map.get("finderType").and_then(|v| v.as_str()),
            Some("PageBack")
        );
        assert_eq!(map.len(), 1);
    }
}
