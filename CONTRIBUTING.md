# Contributing to Flutter MCP

Thank you for your interest in contributing to **Flutter MCP**! 🎉

To maintain code quality, security, and architectural integrity, all contributors must follow this contribution guide.

📖 **[Guía de Contribución en Español](CONTRIBUTING.es.md)**

---

## 🚦 Golden Rules for Contributors

1. **No direct commits to `main`**: All changes must go through a branch and a Pull Request.
2. **Mandatory Quality Gate**: A Pull Request **cannot be merged** unless 100% of automated CI checks pass.
3. **Hexagonal Architecture Preservation**: Code changes must strictly respect layer boundaries.

---

## 🛠️ Step-by-Step Contribution Workflow

### 1. Fork and Clone
Fork the repository on GitHub and clone your fork locally:
```bash
git clone https://github.com/<your-username>/flutter-mcp.git
cd flutter-mcp
```

### 2. Create a Topic Branch
Create a descriptive branch based on `main`:
```bash
# For new features
git checkout -b feature/interactive-sliders

# For bug fixes
git checkout -b fix/screenshot-decoding-timeout
```

### 3. Implement Changes with Architectural Integrity
Ensure your changes follow the Hexagonal Architecture:
- **`src/domain/`**: Pure business logic, entity definitions (`WidgetNode`, `Finder`, `Gesture`), domain services (`TreePruner`). **Zero I/O or network dependencies.**
- **`src/application/`**: Use cases (`FlutterServiceImpl`) and abstract ports (`FlutterAppService`, `FlutterVmPort`).
- **`src/infrastructure/`**: Secondary adapters (WebSocket clients, RPC serializers) and primary adapters (MCP server).

### 4. Run the Local Quality & Test Harness
Before committing, run the complete verification script:
```bash
./scripts/verify_harness.sh
```

This script validates:
1. Code formatting (`cargo fmt --check`).
2. Static linting (`cargo clippy --all-targets -- -D warnings`).
3. Unit tests, Integration tests, and BDD Cucumber/Gherkin tests (`cargo test --all-targets`).
4. Dependency vulnerability scan (`cargo audit`).

### 5. Commit Your Changes
Use Conventional Commits format:
- `feat: add support for flutter_scroll_into_view`
- `fix: resolve timeout exception on slow isolates`
- `docs: update tool parameter reference table`
- `test: add BDD scenario for semantics label search`

### 6. Submit a Pull Request
Push your branch to your fork and submit a PR to `guty3rrez/flutter-mcp:main`:
- Complete the provided Pull Request template checklist.
- Link any related issues (e.g. `Fixes #12`).
- Wait for the GitHub Actions CI workflow to run and ensure all checks pass with a green checkmark.

---

## 🧪 Testing Guidelines

- **Unit Tests**: Place in the corresponding module or `tests/` directory.
- **BDD Tests**: Written in Gherkin feature files under `tests/features/*.feature` and implemented in `tests/bdd.rs`.
- **Integration Tests**: Place in `tests/integration_test.rs` using `MockVmServiceAdapter` to ensure determinism without relying on live Flutter apps.

---

## 📜 Code of Conduct

Please review and adhere to our [Code of Conduct](CODE_OF_CONDUCT.md) in all project interactions.
