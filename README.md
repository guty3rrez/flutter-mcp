# Flutter MCP 🚀

[![CI Quality Gate](https://github.com/guty3rrez/flutter-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/guty3rrez/flutter-mcp/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPLv3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust: 1.80+](https://img.shields.io/badge/Rust-1.80%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![MCP Spec](https://img.shields.io/badge/MCP%20Spec-2024--11--05-green.svg)](https://modelcontextprotocol.io/)

> **High-performance Model Context Protocol (MCP) server written in Rust for native Flutter UI automation, introspection, and End-to-End (E2E) testing.**

📖 **[Versión en Español (Spanish Version)](README.es.md)**

---

## 💡 Why Flutter MCP?

Autonomous AI coding agents (Claude Desktop, Antigravity, Cursor, Windsurf) interact effectively with web applications using browser automation tools like Playwright or Puppeteer. However, **they fail when attempting to interact with native Flutter applications** (Linux Desktop, macOS, Windows, Android, iOS).

Flutter bypasses standard operating system DOM hierarchies and renders UI widgets directly onto a GPU canvas using Skia or Impeller.

**`flutter-mcp` bridges this gap.** Written in Rust with Hexagonal Architecture:
- 🔌 **Direct Dart VM Connection:** Speaks JSON-RPC 2.0 over WebSockets directly to the Flutter engine.
- 🌳 **Zero Layout Noise (`TreePruner`):** Strips away hundreds of boilerplate layout nodes (`Padding`, `SizedBox`, `DecoratedBox`, `Transform`), compressing the UI hierarchy into clean, high-signal semantic tokens tailored for LLM context windows (<2ms pruning).
- ⚡ **Native Gestures & Controls:** Taps, text entry, scrolling, scroll-into-view, async waits, screenshots, and live Hot Reload / Hot Restart.
- 🧪 **True Full-Stack E2E Testing:** Allows AI agents to validate user flows across UI, backend APIs, and local databases (PostgreSQL, SQLite).

---

## 🛠️ MCP Tools Reference (13 Tools)

`flutter-mcp` exposes 13 tools via the Model Context Protocol:

| Tool | Parameters | Description |
| :--- | :--- | :--- |
| `flutter_connect` | `uri: String` | Connect to the running Flutter app's Dart VM Service WebSocket. |
| `flutter_disconnect` | *(none)* | Cleanly disconnect from the active Dart VM Service session. |
| `flutter_snapshot` | *(none)* | Retrieve the pruned, semantic UI widget tree optimized for LLMs. |
| `flutter_tap` | `by: String`, `value: String` | Perform a native tap by `key`, `text`, `tooltip`, `type`, or `semantics`. |
| `flutter_enter_text` | `by: String`, `value: String`, `text: String` | Type text into interactive form fields (`TextField`, `TextFormField`). |
| `flutter_get_text` | `by: String`, `value: String` | Extract visible text content from any widget. |
| `flutter_scroll` | `by`, `value`, `dx`, `dy`, `duration_ms`, `frequency` | Programmatically scroll a scrollable widget (`ListView`, `CustomScrollView`). |
| `flutter_scroll_into_view` | `by`, `value`, `alignment` | Scroll an ancestor container until the target element is visible in the viewport. |
| `flutter_wait_for` | `by`, `value`, `timeout_ms` | Asynchronously wait for a widget to appear in the tree. |
| `flutter_wait_for_absent` | `by`, `value`, `timeout_ms` | Asynchronously wait for a widget to disappear (loading indicators, dialogs). |
| `flutter_screenshot` | `save_path: Option<String>` | Capture a native PNG screenshot directly from the engine framebuffer. |
| `flutter_hot_reload` | *(none)* | Trigger an instant Hot Reload without losing application state. |
| `flutter_hot_restart` | *(none)* | Trigger a complete Hot Restart / Reassemble of the Flutter application. |

---

## 📦 Installation & Setup

### Option 1: Precompiled Binaries (Recommended)
Download the latest precompiled release for your architecture from [GitHub Releases](https://github.com/guty3rrez/flutter-mcp/releases):
- Linux (x86_64)
- macOS (Apple Silicon `aarch64` & Intel `x86_64`)
- Windows (x86_64)

Extract and move the binary to your PATH (e.g. `~/.local/bin/flutter-mcp`).

### Option 2: Install via Cargo
```bash
cargo install --git https://github.com/guty3rrez/flutter-mcp
```

### Option 3: Build from Source
```bash
git clone https://github.com/guty3rrez/flutter-mcp.git
cd flutter-mcp
cargo build --release
cp target/release/flutter-mcp ~/.local/bin/
```

---

## ⚙️ Configuration

### 1. Enable Flutter Driver Extension in Your App
In your Flutter project, ensure `enableFlutterDriverExtension()` is enabled during test/debug mode:

```dart
// lib/main_driver.dart
import 'package:flutter_driver/driver_extension.dart';
import 'package:my_app/main.dart' as app;

void main() {
  enableFlutterDriverExtension();
  app.main();
}
```

Run your application:
```bash
flutter run -d linux -t lib/main_driver.dart
# Note the Dart VM Service URI output, e.g.:
# A Dart VM Service on Linux is available at: ws://127.0.0.1:45678/ws
```

### 2. Configure MCP Clients

#### Claude Desktop (`claude_desktop_config.json`)
```json
{
  "mcpServers": {
    "flutter_mcp": {
      "command": "flutter-mcp",
      "args": []
    }
  }
}
```

#### Claude Code (CLI)
Register the server with the `claude mcp add` command instead of editing a config file by hand:
```bash
claude mcp add flutter_mcp -s user -- /path/to/flutter-mcp
```
- `-s user` makes it available across all your projects; use `-s local` (default) to scope it to the current repo only, or `-s project` to share it via `.mcp.json` with your team.
- Verify it connected with `claude mcp list`.

#### Antigravity CLI / Gemini (`~/.gemini/config/mcp_config.json`)
```json
{
  "mcpServers": {
    "flutter_mcp": {
      "command": "/home/<your-user>/.local/bin/flutter-mcp",
      "args": []
    }
  }
}
```

---

## 🏗️ Architecture

`flutter-mcp` is designed using **Hexagonal Architecture (Ports and Adapters)** to decouple business domain rules from transport protocols and SDK internals:

```mermaid
graph LR
    subgraph Client [MCP Clients]
        Claude[Claude Desktop / Antigravity / Cursor]
    end

    subgraph InboundAdapter [Inbound Adapter]
        MCPServer["FlutterMcpServer (rmcp stdio)"]
    end

    subgraph Core [Domain & Application Core]
        AppPort["FlutterAppService (Inbound Port)"]
        UseCase["FlutterServiceImpl (Use Cases)"]
        Domain["TreePruner | Finder | Gesture"]
        VMPort["FlutterVmPort (Outbound SPI)"]
    end

    subgraph OutboundAdapter [Outbound Adapter]
        WSAdapter["WebSocketVmServiceAdapter (tokio-tungstenite)"]
        MockAdapter["MockVmServiceAdapter (Testing)"]
    end

    subgraph Target [Flutter Engine]
        VM["Dart VM Service (ext.flutter.*)"]
    end

    Client -->|JSON-RPC 2.0 stdio| MCPServer
    MCPServer --> AppPort
    AppPort --> UseCase
    UseCase --> Domain
    UseCase --> VMPort
    VMPort --> WSAdapter
    VMPort -.-> MockAdapter
    WSAdapter -->|JSON-RPC 2.0 WebSockets| VM
```

---

## 🤝 Contributing & Quality Gate

We welcome community contributions! Please review our [Contributing Guide](CONTRIBUTING.md) and [Code of Conduct](CODE_OF_CONDUCT.md).

### Pull Request Rules:
1. **Never commit directly to `main`**: Always branch from `main` (`feature/my-feature` or `fix/issue-description`).
2. **Mandatory Quality Gate**: Every pull request must pass the automated GitHub Actions CI suite:
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --all-targets` (Unit, Integration, and BDD Gherkin tests)
   - `cargo audit`

Run the entire verification suite locally before opening a PR:
```bash
./scripts/verify_harness.sh
```

---

## 📄 License

This project is licensed under the **GNU Affero General Public License v3.0 (AGPLv3)**.  
See the [LICENSE](LICENSE) file for details.
