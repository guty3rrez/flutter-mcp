# Flutter Native MCP (`flutter-native-mcp`)

> **Model Context Protocol (MCP) Server for Native Flutter UI Automation & Inspection**  
> Interact directly with Flutter applications (Desktop, Mobile, Embedded) via Dart VM Service & Service Extensions without relying on browser wrappers or Playwright.

---

## 🚀 Vision

Web testing frameworks like Playwright operate on the HTML DOM tree. When testing Flutter Web, CanvasKit/Skwasm draws directly onto an HTML5 `<canvas>`, making DOM-based selectors virtually useless and brittle. Furthermore, testing desktop apps (Linux, macOS, Windows) or mobile apps (Android, iOS) typically requires heavy native OS accessibility bridges.

`flutter-native-mcp` bridges AI agents directly to the **Flutter Engine & Framework runtime** using the official **Dart VM Service Protocol** and **Flutter Service Extensions**.

### Key Advantages
- 🎯 **Widget & Semantics Awareness**: Query by `Key`, semantic label, widget type, or text directly from Flutter's active `Element` and `RenderObject` trees.
- ⚡ **Native Execution**: Works natively with Linux Desktop GTK apps, Android emulators/devices, macOS, Windows, and Web.
- 🧠 **Agent-Optimized UI Snapshots**: Prunes Flutter's massive diagnostic tree (avoiding context window bloat) into a dense, token-efficient semantic hierarchy.
- 🔄 **Hot Reload Loop**: Agents can write code, trigger `flutter_hot_reload`, and observe instant UI state transitions.

---

## 📖 Specifications & Roadmap

See [SRS.md](file:///home/guty_3rrez/Proyectos/flutter-native-mcp/SRS.md) for the complete Software Requirements Specification and ongoing iterations.
