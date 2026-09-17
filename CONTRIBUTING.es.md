# Guía de Contribución para Flutter MCP

¡Gracias por tu interés en contribuir a **Flutter MCP**! 🎉

Para mantener la calidad del código, la seguridad y la integridad arquitectónica, todos los colaboradores deben seguir esta guía.

📖 **[English Contribution Guide](CONTRIBUTING.md)**

---

## 🚦 Reglas de Oro para Contribuidores

1. **Sin commits directos a `main`**: Todos los cambios deben realizarse a través de ramas y Pull Requests.
2. **Control de Calidad Obligatorio**: Un Pull Request **no podrá ser aprobado ni mergeado** a menos que pase el 100% de los tests automáticos del CI.
3. **Respeto a la Arquitectura Hexagonal**: Las modificaciones deben respetar estrictamente los límites entre capas.

---

## 🛠️ Flujo de Trabajo Paso a Paso

### 1. Fork y Clonación
Haz un Fork del repositorio en GitHub y clona tu fork localmente:
```bash
git clone https://github.com/<tu-usuario>/flutter-mcp.git
cd flutter-mcp
```

### 2. Crear una Rama de Trabajo
Crea una rama descriptiva basada en `main`:
```bash
# Para nuevas funcionalidades
git checkout -b feature/soporte-deslizadores

# Para corrección de errores
git checkout -b fix/timeout-captura-pantalla
```

### 3. Implementar Cambios Respetando la Arquitectura
Asegúrate de que el código siga la Arquitectura Hexagonal:
- **`src/domain/`**: Lógica de negocio pura, entidades (`WidgetNode`, `Finder`, `Gesture`) y servicios de dominio (`TreePruner`). **Cero dependencias de red o I/O.**
- **`src/application/`**: Casos de uso (`FlutterServiceImpl`) y puertos abstractos (`FlutterAppService`, `FlutterVmPort`).
- **`src/infrastructure/`**: Adaptadores secundarios (clientes WebSocket, serializadores RPC) y adaptadores primarios (servidor MCP).

### 4. Ejecutar el Arnés Local de Calidad y Pruebas
Antes de hacer commit, ejecuta el script completo de verificación:
```bash
./scripts/verify_harness.sh
```

Este script valida:
1. Formato del código (`cargo fmt --check`).
2. Análisis estático y linter estricto (`cargo clippy --all-targets -- -D warnings`).
3. Pruebas unitarias, de integración y pruebas BDD con Cucumber/Gherkin (`cargo test --all-targets`).
4. Auditoría de seguridad de dependencias (`cargo audit`).

### 5. Confirmar los Cambios
Usa el estándar de Conventional Commits:
- `feat: agregar soporte para flutter_scroll_into_view`
- `fix: solucionar excepcion de timeout en isolates lentos`
- `docs: actualizar tabla de referencia de parametros`
- `test: anadir escenario BDD para busqueda por semantics`

### 6. Abrir un Pull Request
Sube tu rama a tu fork y abre un Pull Request hacia `guty3rrez/flutter-mcp:main`:
- Completa la lista de verificación de la plantilla de PR.
- Vincula issues relacionados si aplica (ej. `Fixes #12`).
- Espera la ejecución de GitHub Actions y verifica que todos los checks de CI queden en verde.

---

## 🧪 Guías de Pruebas

- **Pruebas Unitarias**: Colocadas en el módulo correspondiente o en la carpeta `tests/`.
- **Pruebas BDD**: Escritas en archivos Gherkin bajo `tests/features/*.feature` e implementadas en `tests/bdd.rs`.
- **Pruebas de Integración**: Ubicadas en `tests/integration_test.rs` usando `MockVmServiceAdapter` para asegurar determinismo sin depender de aplicaciones Flutter externas.

---

## 📜 Código de Conduct

Te invitamos a leer y respetar nuestro [Código de Conducta](CODE_OF_CONDUCT.md) en todas las interacciones del proyecto.
