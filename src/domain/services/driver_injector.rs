/// Lógica pura (sin I/O) para habilitar Flutter Driver en el entrypoint de una app que fue
/// lanzada con su `main.dart` normal, en vez del `main_driver.dart` separado que documenta el
/// README. Separada del adaptador de filesystem para poder testear las transformaciones de texto
/// sin tocar disco, siguiendo el mismo patrón que `LogParser`/`TreePruner`.
/// Estado del entrypoint respecto a Flutter Driver, detectado por `DriverInjector::detect_state`.
/// Reemplaza al viejo `already_enabled: bool` -- ahora hay 4 casos distinguibles, no 2, desde que
/// existe un handler custom propio de flutter-native-mcp (Fase B, key/tooltip/semantics/bounds
/// reales) que puede convivir o no con lo que ya tenga la app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverExtensionState {
    /// Ya llama a `enableFlutterMcpDriverExtension()` -- nada que tocar en el entrypoint (el
    /// archivo generado sí se re-sincroniza, por si esta build trae una versión más nueva).
    UpToDate,
    /// Llama a `enableFlutterDriverExtension();` SIN argumentos (patrón viejo, propio o de una
    /// invocación previa de esta tool antes de Fase B) -- se puede promover con seguridad.
    UpgradableFromBareCall,
    /// Llama a `enableFlutterDriverExtension(` CON argumentos propios (handler/finders/commands
    /// de la app) -- no se toca: perderíamos su configuración.
    CustomHandlerPresent,
    /// No llama a `enableFlutterDriverExtension` en absoluto.
    NotEnabled,
}

pub struct DriverInjector;

impl DriverInjector {
    const OLD_IMPORT: &'static str = "import 'package:flutter_driver/driver_extension.dart';";
    const OLD_BARE_CALL: &'static str = "enableFlutterDriverExtension();";
    const NEW_IMPORT: &'static str = "import 'flutter_mcp_driver_extension.dart';";
    const NEW_CALL: &'static str = "enableFlutterMcpDriverExtension();";

    /// Nombre del archivo generado con el handler custom -- ver `HANDLER_SOURCE`.
    pub const HANDLER_FILE_NAME: &'static str = "flutter_mcp_driver_extension.dart";
    /// Contenido íntegro del handler Dart (Fase B: key/tooltip/semantics/bounds reales vía un
    /// `DataHandler` custom que camina el árbol de `Element`/`RenderObject` en vivo). Única
    /// fuente de verdad, reutilizada por `flutter_start_control` y documentada en README/SKILL.
    pub const HANDLER_SOURCE: &'static str =
        include_str!("../../../assets/flutter_mcp_driver_extension.dart");

    /// Clasifica el estado actual de un entrypoint respecto a Flutter Driver. Ver
    /// `DriverExtensionState` para el significado de cada variante.
    pub fn detect_state(source: &str) -> DriverExtensionState {
        if source.contains("enableFlutterMcpDriverExtension(") {
            DriverExtensionState::UpToDate
        } else if source.contains(Self::OLD_BARE_CALL) {
            DriverExtensionState::UpgradableFromBareCall
        } else if source.contains("enableFlutterDriverExtension(") {
            DriverExtensionState::CustomHandlerPresent
        } else {
            DriverExtensionState::NotEnabled
        }
    }

    /// Inyecta el import y una llamada a `enableFlutterMcpDriverExtension()` como primera
    /// sentencia de `main()`. Devuelve `None` si no se encontró una función `main`
    /// reconocible (`main(`, con boundary de identificador, seguido de un `{`). No-op seguro
    /// (devuelve el source sin cambios) si el estado ya no es `NotEnabled`.
    pub fn inject(source: &str) -> Option<String> {
        if Self::detect_state(source) != DriverExtensionState::NotEnabled {
            return Some(source.to_string());
        }

        let with_import = Self::insert_import(source);
        Self::insert_enable_call(&with_import)
    }

    /// Promueve un entrypoint en estado `UpgradableFromBareCall` (llamada vieja sin handler) al
    /// patrón nuevo: reemplaza el import y la llamada viejos por los nuevos. Reemplazo de
    /// substring exacto y acotado (no un parser Dart) -- seguro porque `detect_state` ya
    /// garantizó que el texto exacto está presente y que no hay argumentos custom que preservar.
    /// Devuelve `None` si el estado no es `UpgradableFromBareCall`.
    pub fn upgrade(source: &str) -> Option<String> {
        if Self::detect_state(source) != DriverExtensionState::UpgradableFromBareCall {
            return None;
        }
        Some(
            source
                .replacen(Self::OLD_IMPORT, Self::NEW_IMPORT, 1)
                .replacen(Self::OLD_BARE_CALL, Self::NEW_CALL, 1),
        )
    }

    fn insert_import(source: &str) -> String {
        let last_import_line = source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with("import "))
            .last();

        match last_import_line {
            Some((idx, _)) => {
                let mut lines: Vec<&str> = source.lines().collect();
                lines.insert(idx + 1, Self::NEW_IMPORT);
                lines.join("\n")
            }
            None => format!("{}\n\n{source}", Self::NEW_IMPORT),
        }
    }

    fn insert_enable_call(source: &str) -> Option<String> {
        let brace_idx = Self::find_main_brace_index(source)?;
        let mut result = String::with_capacity(source.len() + Self::NEW_CALL.len() + 4);
        result.push_str(&source[..=brace_idx]);
        result.push_str("\n  ");
        result.push_str(Self::NEW_CALL);
        result.push_str(&source[brace_idx + 1..]);
        Some(result)
    }

    /// Busca el índice del primer `{` que abre el cuerpo de una función `main` de nivel
    /// superior (`void main()`, `void main() async`, `Future<void> main() async`, etc.),
    /// evitando falsos positivos como `domain(` gracias al chequeo de boundary de identificador.
    fn find_main_brace_index(source: &str) -> Option<usize> {
        let bytes = source.as_bytes();
        let mut search_from = 0;

        while let Some(rel_idx) = source[search_from..].find("main(") {
            let idx = search_from + rel_idx;
            let is_boundary =
                idx == 0 || !matches!(bytes[idx - 1] as char, c if c.is_alphanumeric() || c == '_');

            if is_boundary && let Some(rel_brace) = source[idx..].find('{') {
                return Some(idx + rel_brace);
            }

            search_from = idx + "main(".len();
        }

        None
    }
}

/// Edición mínima de `pubspec.yaml` para asegurar que `flutter_driver` esté declarado como
/// dependencia. No es un parser YAML completo: opera línea a línea, suficiente para este único
/// caso de uso (una entrada simple bajo `dev_dependencies:`).
pub struct PubspecEditor;

impl PubspecEditor {
    const ENTRY: &'static str = "  flutter_driver:\n    sdk: flutter";

    /// True si `pubspec.yaml` ya declara `flutter_driver` como dependencia (bajo
    /// `dependencies:` o `dev_dependencies:`, no importa cuál).
    pub fn declares_flutter_driver(source: &str) -> bool {
        source
            .lines()
            .any(|line| line.trim_start().starts_with("flutter_driver:"))
    }

    /// Agrega `flutter_driver` como `dev_dependency` (`sdk: flutter`) si todavía no está
    /// declarado. Reutiliza la sección `dev_dependencies:` existente si la encuentra, o la crea
    /// al final del archivo si no existe ninguna.
    pub fn add_flutter_driver_dev_dependency(source: &str) -> String {
        if Self::declares_flutter_driver(source) {
            return source.to_string();
        }

        match source.find("dev_dependencies:") {
            Some(idx) => {
                let line_end = source[idx..]
                    .find('\n')
                    .map(|i| idx + i + 1)
                    .unwrap_or(source.len());
                let mut result = String::with_capacity(source.len() + Self::ENTRY.len() + 1);
                result.push_str(&source[..line_end]);
                result.push_str(Self::ENTRY);
                result.push('\n');
                result.push_str(&source[line_end..]);
                result
            }
            None => {
                let mut result = source.to_string();
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("\ndev_dependencies:\n");
                result.push_str(Self::ENTRY);
                result.push('\n');
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_MAIN: &str =
        "import 'package:flutter/material.dart';\n\nvoid main() {\n  runApp(const MyApp());\n}\n";

    const OLD_BARE_SOURCE: &str = "import 'package:flutter_driver/driver_extension.dart';\n\nvoid main() {\n  enableFlutterDriverExtension();\n  runApp(const MyApp());\n}\n";
    const CUSTOM_HANDLER_SOURCE: &str = "import 'package:flutter_driver/driver_extension.dart';\n\nvoid main() {\n  enableFlutterDriverExtension(handler: myHandler);\n  runApp(const MyApp());\n}\n";
    const UP_TO_DATE_SOURCE: &str = "import 'flutter_mcp_driver_extension.dart';\n\nvoid main() {\n  enableFlutterMcpDriverExtension();\n  runApp(const MyApp());\n}\n";

    #[test]
    fn test_detect_state_classifies_all_four_cases() {
        assert_eq!(
            DriverInjector::detect_state(SIMPLE_MAIN),
            DriverExtensionState::NotEnabled
        );
        assert_eq!(
            DriverInjector::detect_state(OLD_BARE_SOURCE),
            DriverExtensionState::UpgradableFromBareCall
        );
        assert_eq!(
            DriverInjector::detect_state(CUSTOM_HANDLER_SOURCE),
            DriverExtensionState::CustomHandlerPresent
        );
        assert_eq!(
            DriverInjector::detect_state(UP_TO_DATE_SOURCE),
            DriverExtensionState::UpToDate
        );
    }

    #[test]
    fn test_inject_adds_import_and_call_before_run_app() {
        let injected = DriverInjector::inject(SIMPLE_MAIN).expect("debe inyectar");

        assert!(injected.contains("import 'flutter_mcp_driver_extension.dart';"));
        let call_pos = injected
            .find("enableFlutterMcpDriverExtension();")
            .expect("debe contener la llamada");
        let run_app_pos = injected.find("runApp(").expect("debe contener runApp");
        assert!(
            call_pos < run_app_pos,
            "enableFlutterMcpDriverExtension() debe ir antes de runApp()"
        );
    }

    #[test]
    fn test_inject_is_idempotent_when_already_enabled() {
        let result = DriverInjector::inject(UP_TO_DATE_SOURCE).expect("debe ser no-op exitoso");
        assert_eq!(result, UP_TO_DATE_SOURCE);

        let result = DriverInjector::inject(CUSTOM_HANDLER_SOURCE).expect("debe ser no-op");
        assert_eq!(result, CUSTOM_HANDLER_SOURCE);
    }

    #[test]
    fn test_inject_supports_async_main_and_ignores_domain_false_positive() {
        let source = "import 'package:flutter/material.dart';\n\nString domain() => 'x';\n\nFuture<void> main() async {\n  runApp(const MyApp());\n}\n";
        let injected = DriverInjector::inject(source).expect("debe inyectar en main async");
        assert!(injected.contains("enableFlutterMcpDriverExtension();"));
        // No debe haber insertado nada dentro de `domain()`
        let domain_pos = injected.find("domain()").unwrap();
        let call_pos = injected.find("enableFlutterMcpDriverExtension();").unwrap();
        assert!(call_pos > domain_pos);
    }

    #[test]
    fn test_inject_returns_none_without_main_function() {
        let source = "class Foo {}\n";
        assert_eq!(DriverInjector::inject(source), None);
    }

    #[test]
    fn test_inject_prepends_import_when_no_existing_imports() {
        let source = "void main() {\n  print('hi');\n}\n";
        let injected = DriverInjector::inject(source).expect("debe inyectar");
        assert!(injected.starts_with("import 'flutter_mcp_driver_extension.dart';\n\nvoid main()"));
    }

    #[test]
    fn test_upgrade_promotes_bare_call_to_new_handler() {
        let upgraded = DriverInjector::upgrade(OLD_BARE_SOURCE).expect("debe promover");
        assert!(upgraded.contains("import 'flutter_mcp_driver_extension.dart';"));
        assert!(upgraded.contains("enableFlutterMcpDriverExtension();"));
        assert!(!upgraded.contains("package:flutter_driver/driver_extension.dart"));
        assert!(!upgraded.contains("enableFlutterDriverExtension();"));
        // El resto del archivo (runApp) queda intacto.
        assert!(upgraded.contains("runApp(const MyApp());"));
    }

    #[test]
    fn test_upgrade_returns_none_for_non_upgradable_states() {
        assert_eq!(DriverInjector::upgrade(SIMPLE_MAIN), None);
        assert_eq!(DriverInjector::upgrade(CUSTOM_HANDLER_SOURCE), None);
        assert_eq!(DriverInjector::upgrade(UP_TO_DATE_SOURCE), None);
    }

    #[test]
    fn test_pubspec_declares_flutter_driver() {
        let with_dep = "dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n";
        assert!(PubspecEditor::declares_flutter_driver(with_dep));

        let without_dep = "dev_dependencies:\n  flutter_test:\n    sdk: flutter\n";
        assert!(!PubspecEditor::declares_flutter_driver(without_dep));
    }

    #[test]
    fn test_add_flutter_driver_dev_dependency_reuses_existing_section() {
        let source = "name: my_app\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n";
        let result = PubspecEditor::add_flutter_driver_dev_dependency(source);

        assert!(result.contains("flutter_driver:\n    sdk: flutter"));
        assert!(result.contains("flutter_test:\n    sdk: flutter"));
        // La entrada nueva debe estar inmediatamente después del encabezado de la sección
        let section_pos = result.find("dev_dependencies:").unwrap();
        let new_entry_pos = result.find("flutter_driver:").unwrap();
        let old_entry_pos = result.find("flutter_test:").unwrap();
        assert!(section_pos < new_entry_pos);
        assert!(new_entry_pos < old_entry_pos);
    }

    #[test]
    fn test_add_flutter_driver_dev_dependency_creates_section_if_missing() {
        let source = "name: my_app\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n";
        let result = PubspecEditor::add_flutter_driver_dev_dependency(source);

        assert!(result.contains("dev_dependencies:\n  flutter_driver:\n    sdk: flutter"));
    }

    #[test]
    fn test_add_flutter_driver_dev_dependency_is_idempotent() {
        let source = "dev_dependencies:\n  flutter_driver:\n    sdk: flutter\n";
        let result = PubspecEditor::add_flutter_driver_dev_dependency(source);
        assert_eq!(result, source);
    }
}
