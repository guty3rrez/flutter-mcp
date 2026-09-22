use crate::domain::entities::{Finder, WidgetNode};

/// Cantidad máxima de candidatos sugeridos cuando un finder no matchea nada -- suficiente para
/// orientar sin inundar el mensaje de error.
const MAX_CANDIDATES: usize = 5;

/// Resultado de resolver un finder contra un árbol de widgets ya obtenido (sin ida y vuelta
/// adicional al VM Service) -- usado como pre-chequeo rápido antes de despachar un comando de
/// Flutter Driver con un finder `text`/`tooltip`/`semantics`, que de otro modo bloquea hasta
/// agotar su timeout completo cuando no hay match real (ver `WebSocketVmServiceAdapter::
/// precheck_finder`).
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// Al menos un nodo del árbol matchea el finder -- se espera que Flutter Driver resuelva
    /// casi al instante.
    Matched,
    /// Ningún nodo matchea. `candidates` son hasta `MAX_CANDIDATES` nodos con
    /// texto/tooltip/semantics parecidos al valor buscado, útiles para sugerir en el error.
    NotFound { candidates: Vec<WidgetNode> },
}

pub struct FinderResolver;

impl FinderResolver {
    /// `true` para los finders "lentos" -- `Text`/`Tooltip`/`SemanticsLabel` -- que dependen de
    /// que el string buscado coincida con algo del árbol y por eso son susceptibles al wait
    /// implícito de Flutter Driver cuando no matchean. `Key`/`Type`/`Coordinates` son exactos y
    /// baratos, no necesitan pre-chequeo.
    pub fn is_slow_finder(finder: &Finder) -> bool {
        matches!(
            finder,
            Finder::Text { .. } | Finder::Tooltip(_) | Finder::SemanticsLabel(_)
        )
    }

    /// Resuelve `finder` contra `root` (la raíz de un árbol ya podado por `TreePruner`).
    /// El criterio de match replica lo que `Finder::to_driver_params()` efectivamente le manda
    /// a Flutter Driver hoy: comparación exacta (case-sensitive, tras normalizar comillas/
    /// espacios) de todo el string -- `Finder::Text.exact` no se usa acá a propósito porque
    /// tampoco lo usa `to_driver_params()` (ver test `to_driver_params_ignores_exact_flag`).
    /// Si esa paridad cambia, hay que actualizar ambos lugares juntos.
    pub fn quick_resolve(root: &WidgetNode, finder: &Finder) -> ResolveOutcome {
        if Self::any_match(root, finder) {
            return ResolveOutcome::Matched;
        }

        let mut candidates = Vec::new();
        Self::collect_candidates(root, finder, &mut candidates);
        candidates.truncate(MAX_CANDIDATES);
        ResolveOutcome::NotFound { candidates }
    }

    fn any_match(node: &WidgetNode, finder: &Finder) -> bool {
        Self::node_matches(node, finder) || node.children.iter().any(|c| Self::any_match(c, finder))
    }

    fn node_matches(node: &WidgetNode, finder: &Finder) -> bool {
        match finder {
            Finder::Text { text, .. } => matches_normalized(node.text.as_deref(), text),
            Finder::Tooltip(tip) => matches_normalized(node.tooltip.as_deref(), tip),
            Finder::SemanticsLabel(label) => {
                matches_normalized(node.semantics_label.as_deref(), label)
            }
            _ => false,
        }
    }

    fn collect_candidates(node: &WidgetNode, finder: &Finder, out: &mut Vec<WidgetNode>) {
        if out.len() >= MAX_CANDIDATES {
            return;
        }
        if Self::is_similar(node, finder) {
            out.push(node.clone());
        }
        for child in &node.children {
            Self::collect_candidates(child, finder, out);
        }
    }

    fn is_similar(node: &WidgetNode, finder: &Finder) -> bool {
        let needle = normalize(Self::finder_value(finder)).to_lowercase();
        if needle.is_empty() {
            return false;
        }
        [&node.text, &node.tooltip, &node.semantics_label]
            .into_iter()
            .flatten()
            .any(|field| {
                let haystack = normalize(field).to_lowercase();
                !haystack.is_empty() && (haystack.contains(&needle) || needle.contains(&haystack))
            })
    }

    fn finder_value(finder: &Finder) -> &str {
        match finder {
            Finder::Text { text, .. } => text,
            Finder::Tooltip(t) => t,
            Finder::SemanticsLabel(l) => l,
            _ => "",
        }
    }
}

fn matches_normalized(node_value: Option<&str>, needle: &str) -> bool {
    node_value
        .map(normalize)
        .is_some_and(|v| v == normalize(needle))
}

/// `TreePruner` a veces preserva las comillas literales de la descripción cruda de Flutter
/// (ej. `"\"Guardar\""` cuando viene de una propiedad `data`/`text`/`title` genérica) y a veces
/// no (ver el branch especial para widgets `Text` en `tree_pruner.rs`) -- normalizamos
/// recortando comillas envolventes y espacios para no depender de esa inconsistencia.
fn normalize(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(widget_type: &str) -> WidgetNode {
        WidgetNode::new(widget_type, false)
    }

    #[test]
    fn to_driver_params_ignores_exact_flag() {
        // Documenta (y protege) la premisa de la que depende `quick_resolve`: hoy
        // `Finder::to_driver_params()` no distingue `exact: true` de `exact: false` -- si esto
        // cambia, `quick_resolve` debe revisarse para no divergir del protocolo real.
        assert_eq!(
            Finder::by_text("Hola", true).to_driver_params(),
            Finder::by_text("Hola", false).to_driver_params()
        );
    }

    #[test]
    fn is_slow_finder_true_for_text_tooltip_semantics() {
        assert!(FinderResolver::is_slow_finder(&Finder::by_text("x", true)));
        assert!(FinderResolver::is_slow_finder(&Finder::by_tooltip("x")));
        assert!(FinderResolver::is_slow_finder(&Finder::by_semantics_label(
            "x"
        )));
    }

    #[test]
    fn is_slow_finder_false_for_key_type_coordinates() {
        assert!(!FinderResolver::is_slow_finder(&Finder::by_key("x")));
        assert!(!FinderResolver::is_slow_finder(&Finder::by_type("x")));
        assert!(!FinderResolver::is_slow_finder(&Finder::Coordinates {
            x: 1.0,
            y: 1.0
        }));
    }

    #[test]
    fn quick_resolve_matches_exact_text_even_with_quoted_property_description() {
        let mut root = leaf("Scaffold");
        let mut button = leaf("ElevatedButton");
        button.add_child(leaf("Text").with_text("\"Guardar\""));
        root.add_child(button);

        let outcome = FinderResolver::quick_resolve(&root, &Finder::by_text("Guardar", true));
        assert_eq!(outcome, ResolveOutcome::Matched);
    }

    #[test]
    fn quick_resolve_matches_tooltip_and_semantics_label() {
        let mut root = leaf("Scaffold");
        root.add_child(leaf("IconButton").with_tooltip("Cerrar sesión"));
        root.add_child(leaf("Checkbox").with_semantics_label("Aceptar términos"));

        assert_eq!(
            FinderResolver::quick_resolve(&root, &Finder::by_tooltip("Cerrar sesión")),
            ResolveOutcome::Matched
        );
        assert_eq!(
            FinderResolver::quick_resolve(&root, &Finder::by_semantics_label("Aceptar términos")),
            ResolveOutcome::Matched
        );
    }

    #[test]
    fn quick_resolve_not_found_returns_similar_candidates() {
        let mut root = leaf("Scaffold");
        root.add_child(leaf("Text").with_text("Guardar cambios"));
        root.add_child(leaf("Text").with_text("Cancelar"));

        let outcome = FinderResolver::quick_resolve(&root, &Finder::by_text("Guardar", true));
        match outcome {
            ResolveOutcome::NotFound { candidates } => {
                assert_eq!(candidates.len(), 1);
                assert_eq!(candidates[0].text.as_deref(), Some("Guardar cambios"));
            }
            ResolveOutcome::Matched => panic!("no debería matchear un substring como exacto"),
        }
    }

    #[test]
    fn quick_resolve_not_found_without_any_similar_text_yields_no_candidates() {
        let mut root = leaf("Scaffold");
        root.add_child(leaf("Text").with_text("Algo completamente distinto"));

        let outcome = FinderResolver::quick_resolve(&root, &Finder::by_text("Guardar", true));
        assert_eq!(
            outcome,
            ResolveOutcome::NotFound {
                candidates: Vec::new()
            }
        );
    }
}
