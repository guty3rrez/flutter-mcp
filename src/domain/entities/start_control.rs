/// Resultado de `flutter_start_control`: qué se tuvo que tocar para dejar la app controlable.
#[derive(Debug, Clone, PartialEq)]
pub struct StartControlOutcome {
    /// Ruta (project_root + entrypoint) del archivo evaluado/parchado.
    pub entrypoint_path: String,
    /// True si el entrypoint ya tenía Flutter Driver habilitado antes de esta llamada.
    pub already_enabled: bool,
    /// True si hubo que agregar `flutter_driver` a `pubspec.yaml`. Cuando esto pasa, la
    /// operación se detiene ahí: la dependencia no está resuelta todavía (`pubspec.lock`),
    /// así que ni el Hot Restart puede completar la inyección.
    pub pubspec_updated: bool,
    /// True si se disparó Hot Restart en el isolate principal.
    pub hot_restart_triggered: bool,
    /// True si, tras el Hot Restart, se revirtió el entrypoint a su contenido original en disco.
    pub reverted: bool,
}
