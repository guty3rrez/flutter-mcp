use crate::domain::entities::WidgetNode;
use serde_json::Value;

/// Lista de tipos de widgets conocidos como interactivos en Flutter
const INTERACTIVE_WIDGETS: &[&str] = &[
    "ElevatedButton",
    "TextButton",
    "OutlinedButton",
    "IconButton",
    "FloatingActionButton",
    "InkWell",
    "GestureDetector",
    "Switch",
    "Checkbox",
    "Radio",
    "Slider",
    "TextField",
    "TextFormField",
    "DropdownButton",
    "PopupMenuButton",
    "CupertinoButton",
    "CupertinoSwitch",
    "CupertinoTextField",
];

/// Lista de widgets puramente estructurales o de layout que no aportan valor semántico directo al LLM
const LAYOUT_NOISE_WIDGETS: &[&str] = &[
    "Padding",
    "SizedBox",
    "ConstrainedBox",
    "ColoredBox",
    "Center",
    "Align",
    "Transform",
    "ClipRRect",
    "ClipRect",
    "DecoratedBox",
    "DefaultTextStyle",
    "Directionality",
    "MediaQuery",
    "Theme",
    "Builder",
    "KeyedSubtree",
    "RepaintBoundary",
    "CustomPaint",
    "AnimatedBuilder",
    "NotificationListener",
    "ScrollConfiguration",
    "Focus",
    "FocusScope",
    "Actions",
    "Shortcuts",
];

pub struct TreePruner;

impl TreePruner {
    /// Poda un árbol de diagnósticos crudo de Flutter (JSON de ext.flutter.inspector)
    /// convirtiéndolo en un árbol compacto de `WidgetNode`
    pub fn prune_diagnostics_tree(json: &Value) -> Option<WidgetNode> {
        let json = json.get("result").unwrap_or(json);

        let widget_type = json
            .get("description")
            .and_then(|d| d.as_str())
            .or_else(|| json.get("widgetRuntimeType").and_then(|t| t.as_str()))
            .unwrap_or("Unknown")
            .to_string();

        let properties = json.get("properties").and_then(|p| p.as_array());

        // Extraer Key
        let key = properties.and_then(|props| {
            props.iter().find_map(|prop| {
                if prop.get("name").and_then(|n| n.as_str()) == Some("key") {
                    prop.get("description")
                        .and_then(|d| d.as_str())
                        .map(ToString::to_string)
                } else {
                    None
                }
            })
        });

        // Extraer Texto
        let text = properties.and_then(|props| {
            props.iter().find_map(|prop| {
                let name = prop.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name == "data" || name == "text" {
                    prop.get("description")
                        .and_then(|d| d.as_str())
                        .map(ToString::to_string)
                } else {
                    None
                }
            })
        });

        // Extraer Tooltip / Semantics
        let tooltip = properties.and_then(|props| {
            props.iter().find_map(|prop| {
                let name = prop.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name == "tooltip" || name == "message" {
                    prop.get("description")
                        .and_then(|d| d.as_str())
                        .map(ToString::to_string)
                } else {
                    None
                }
            })
        });

        let is_interactive = INTERACTIVE_WIDGETS.contains(&widget_type.as_str())
            || json
                .get("hasTapHandler")
                .and_then(|h| h.as_bool())
                .unwrap_or(false);

        let is_noise = LAYOUT_NOISE_WIDGETS.contains(&widget_type.as_str());

        // Procesar hijos recursivamente
        let mut pruned_children = Vec::new();
        if let Some(children) = json.get("children").and_then(|c| c.as_array()) {
            for child in children {
                if let Some(pruned_child) = Self::prune_diagnostics_tree(child) {
                    pruned_children.push(pruned_child);
                }
            }
        }

        // Si es ruido de maquetación, pero tiene una Key explícita o texto, lo conservamos.
        // Si no, "aplanamos" sus hijos pasándolos hacia arriba.
        if is_noise && key.is_none() && text.is_none() {
            if pruned_children.len() == 1 {
                return pruned_children.into_iter().next();
            } else if pruned_children.is_empty() {
                return None;
            }
        }

        let mut node = WidgetNode::new(widget_type, is_interactive);
        if let Some(k) = key {
            node = node.with_key(k);
        }
        if let Some(t) = text {
            node = node.with_text(t);
        }
        if let Some(tp) = tooltip {
            node = node.with_tooltip(tp);
        }
        node.children = pruned_children;

        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_prunes_single_padding_noise_with_child() {
        let input = json!({
            "description": "Padding",
            "children": [
                {
                    "description": "ElevatedButton",
                    "properties": [
                        {"name": "key", "description": "[<'btn_login'>]"}
                    ],
                    "children": [
                        {
                            "description": "Text",
                            "properties": [
                                {"name": "data", "description": "\"Iniciar Sesión\""}
                            ]
                        }
                    ]
                }
            ]
        });

        let pruned = TreePruner::prune_diagnostics_tree(&input).expect("Should not be None");
        assert_eq!(pruned.widget_type, "ElevatedButton");
        assert_eq!(pruned.key.as_deref(), Some("[<'btn_login'>]"));
        assert!(pruned.is_interactive);
        assert_eq!(pruned.children.len(), 1);
        assert_eq!(pruned.children[0].widget_type, "Text");
        assert_eq!(
            pruned.children[0].text.as_deref(),
            Some("\"Iniciar Sesión\"")
        );
    }

    #[test]
    fn test_discards_empty_noise_node() {
        let input = json!({
            "description": "SizedBox",
            "children": []
        });

        let pruned = TreePruner::prune_diagnostics_tree(&input);
        assert!(pruned.is_none());
    }
}
