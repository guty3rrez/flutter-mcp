/// Lógica pura (sin I/O) para habilitar Flutter Driver en el entrypoint de una app que fue
/// lanzada con su `main.dart` normal, en vez del `main_driver.dart` separado que documenta el
/// README. Separada del adaptador de filesystem para poder testear las transformaciones de texto
/// sin tocar disco, siguiendo el mismo patrón que `LogParser`/`TreePruner`.
pub struct DriverInjector;

impl DriverInjector {
    const DRIVER_IMPORT: &'static str = "import 'package:flutter_driver/driver_extension.dart';";
    const ENABLE_CALL: &'static str = "enableFlutterDriverExtension();";

    /// True si el entrypoint ya tiene Flutter Driver habilitado (llamada agregada por esta
    /// tool en una invocación previa, o a mano por el propio usuario).
    pub fn already_enabled(source: &str) -> bool {
        source.contains("enableFlutterDriverExtension(")
    }

    /// Inyecta el import y una llamada a `enableFlutterDriverExtension()` como primera
    /// sentencia de `main()`. Devuelve `None` si no se encontró una función `main`
    /// reconocible (`main(`, con boundary de identificador, seguido de un `{`).
    pub fn inject(source: &str) -> Option<String> {
        if Self::already_enabled(source) {
            return Some(source.to_string());
        }

        let with_import = Self::insert_import(source);
        Self::insert_enable_call(&with_import)
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
                lines.insert(idx + 1, Self::DRIVER_IMPORT);
                lines.join("\n")
            }
            None => format!("{}\n\n{source}", Self::DRIVER_IMPORT),
        }
    }

    fn insert_enable_call(source: &str) -> Option<String> {
        let brace_idx = Self::find_main_brace_index(source)?;
        let mut result = String::with_capacity(source.len() + Self::ENABLE_CALL.len() + 4);
        result.push_str(&source[..=brace_idx]);
        result.push_str("\n  ");
        result.push_str(Self::ENABLE_CALL);
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

    #[test]
    fn test_already_enabled_detects_existing_call() {
        assert!(!DriverInjector::already_enabled(SIMPLE_MAIN));
        assert!(DriverInjector::already_enabled(
            "void main() {\n  enableFlutterDriverExtension();\n  runApp(const MyApp());\n}\n"
        ));
    }

    #[test]
    fn test_inject_adds_import_and_call_before_run_app() {
        let injected = DriverInjector::inject(SIMPLE_MAIN).expect("debe inyectar");

        assert!(injected.contains("import 'package:flutter_driver/driver_extension.dart';"));
        let call_pos = injected
            .find("enableFlutterDriverExtension();")
            .expect("debe contener la llamada");
        let run_app_pos = injected.find("runApp(").expect("debe contener runApp");
        assert!(
            call_pos < run_app_pos,
            "enableFlutterDriverExtension() debe ir antes de runApp()"
        );
    }

    #[test]
    fn test_inject_is_idempotent_when_already_enabled() {
        let already = "import 'package:flutter_driver/driver_extension.dart';\n\nvoid main() {\n  enableFlutterDriverExtension();\n  runApp(const MyApp());\n}\n";
        let result = DriverInjector::inject(already).expect("debe ser no-op exitoso");
        assert_eq!(result, already);
    }

    #[test]
    fn test_inject_supports_async_main_and_ignores_domain_false_positive() {
        let source = "import 'package:flutter/material.dart';\n\nString domain() => 'x';\n\nFuture<void> main() async {\n  runApp(const MyApp());\n}\n";
        let injected = DriverInjector::inject(source).expect("debe inyectar en main async");
        assert!(injected.contains("enableFlutterDriverExtension();"));
        // No debe haber insertado nada dentro de `domain()`
        let domain_pos = injected.find("domain()").unwrap();
        let call_pos = injected.find("enableFlutterDriverExtension();").unwrap();
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
        assert!(
            injected.starts_with(
                "import 'package:flutter_driver/driver_extension.dart';\n\nvoid main()"
            )
        );
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
