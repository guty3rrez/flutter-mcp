// Generado/documentado por flutter-native-mcp (flutter_start_control lo escribe
// automáticamente en lib/flutter_mcp_driver_extension.dart; para setup manual, ver README.md/
// README.es.md, sección "Enable Flutter Driver Extension in Your App").
//
// Expone key/tooltip/semantics_label/bounds REALES en flutter_snapshot -- ver issue
// 4896ac04-b2c3-46d9-a943-3fdaa03909f8 ("Fase B"). No contiene lógica de tu app: es seguro de
// commitear. Solo se importa desde un entrypoint de driver (lib/main_driver.dart, o el
// lib/main.dart parcheado en caliente por flutter_start_control) -- nunca desde un build de
// release.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_driver/driver_extension.dart';

/// Namespace del protocolo custom sobre `FlutterDriver.requestData`. Reduce (sin eliminar del
/// todo -- ver README, sección "Compatibilidad y riesgos") la chance de choque con un
/// `DataHandler` propio que la app ya tuviera registrado para sus propios fines de testing.
/// "v1": si el shape de este protocolo cambia de forma incompatible en el futuro, sube a v2 y el
/// lado Rust puede negociar/probar ambos sin romper apps que no actualizaron este archivo.
const String _kFlutterMcpNamespace = '__flutter_mcp_v1__';
const String _kGetRichTreeCommand = '$_kFlutterMcpNamespace:get_rich_tree';

/// Texto EXACTO que Flutter Driver devuelve cuando `requestData` se llama sin ningún
/// `DataHandler` registrado (ver `handler_factory.dart::_requestData` en el SDK de Flutter).
/// flutter-native-mcp lo usa para distinguir "entrypoint viejo, sin este handler" de un error
/// real. NO renombrar sin sincronizar con `NO_REQUEST_DATA_HANDLER_SENTINEL` en
/// `vm_service_client.rs`.
const String kFlutterMcpNoHandlerSentinel = 'No requestData Extension registered';

SemanticsHandle? _mcpSemanticsHandle;

/// Fuerza la construcción del árbol de semantics (gateado por defecto -- `RenderObject.
/// debugSemantics` siempre da null sin esto) y espera un frame para que termine de construirse,
/// igual que el comando nativo `set_semantics` de Flutter Driver. Idempotente: una vez habilitado
/// queda habilitado el resto de la sesión (el `SemanticsHandle` se retiene en `_mcpSemanticsHandle`
/// para que no se libere).
Future<void> _ensureSemanticsForInspection() async {
  if (_mcpSemanticsHandle != null) return;
  final bool wasEnabled = SemanticsBinding.instance.semanticsEnabled;
  _mcpSemanticsHandle = SemanticsBinding.instance.ensureSemantics();
  if (!wasEnabled) {
    final completer = Completer<void>();
    SchedulerBinding.instance.addPostFrameCallback((_) => completer.complete());
    await completer.future;
  }
}

/// Nodo intermedio mutable usado durante el walk, antes de serializar al shape JSON que
/// `TreePruner` (lado Rust, `src/domain/services/tree_pruner.rs`) ya sabe parsear sin cambios.
class _NodeDraft {
  _NodeDraft(this.description);
  final String description;
  String? key;
  String? tooltip;
  String? semanticsLabel;
  String? textPreview;
  Map<String, double>? bounds;
  final List<_NodeDraft> children = <_NodeDraft>[];

  /// True si este nodo no aporta nada propio y tiene exactamente un hijo -- candidato a
  /// aplanarse. Deliberadamente estructural, no una lista de tipos: así cubre wrappers internos
  /// de Flutter (Semantics, MergeSemantics, RepaintBoundary, Builder, _FocusMarker, etc.) sin
  /// tener que enumerarlos.
  bool get isFlattenableWrapper =>
      key == null &&
      tooltip == null &&
      semanticsLabel == null &&
      textPreview == null &&
      children.length == 1;

  Map<String, dynamic> toJson() {
    final map = <String, dynamic>{'description': description};
    final properties = <Map<String, String>>[];
    if (key != null) properties.add({'name': 'key', 'description': key!});
    if (tooltip != null) properties.add({'name': 'tooltip', 'description': tooltip!});
    if (semanticsLabel != null) {
      properties.add({'name': 'semanticsLabel', 'description': semanticsLabel!});
    }
    if (properties.isNotEmpty) map['properties'] = properties;
    if (textPreview != null) map['textPreview'] = textPreview;
    if (bounds != null) map['bounds'] = bounds;
    if (children.isNotEmpty) {
      map['children'] = children.map((c) => c.toJson()).toList();
    }
    return map;
  }
}

_NodeDraft _describeElement(Element element) {
  final Widget widget = element.widget;
  final draft = _NodeDraft(widget.runtimeType.toString());

  final Key? key = widget.key;
  if (key != null) {
    draft.key = key.toString();
  }

  if (widget is Tooltip) {
    // IconButton envuelve internamente a su hijo en un Tooltip cuando se le pasa `tooltip:` --
    // no hace falta un caso especial por tipo de widget, el walk llega a este nodo solo.
    // `message` es `String?` -- `Tooltip.richMessage` (no soportado acá) puede ser la única
    // fuente de contenido, en cuyo caso `message` queda null.
    final String? message = widget.message;
    if (message != null && message.isNotEmpty) {
      draft.tooltip = message;
    }
  }

  if (widget is Text && widget.data != null) {
    draft.textPreview = widget.data;
  } else if (widget is EditableText) {
    draft.textPreview = widget.controller.text;
  }

  final RenderObject? renderObject = element.renderObject;
  if (renderObject != null) {
    final String? semanticsLabel = renderObject.debugSemantics?.label;
    if (semanticsLabel != null && semanticsLabel.isNotEmpty) {
      draft.semanticsLabel = semanticsLabel;
    }
    if (renderObject is RenderBox && renderObject.hasSize) {
      try {
        final Offset topLeft = renderObject.localToGlobal(Offset.zero);
        draft.bounds = <String, double>{
          'left': topLeft.dx,
          'top': topLeft.dy,
          'width': renderObject.size.width,
          'height': renderObject.size.height,
        };
      } catch (_) {
        // localToGlobal puede lanzar si el objeto todavía no está atado al árbol de layout
        // (build en curso a mitad del walk) -- se omite el bounds de ESTE nodo puntual, no se
        // aborta el árbol completo por eso.
      }
    }
  }

  element.visitChildren((Element child) {
    final _NodeDraft childDraft = _describeElement(child);
    if (childDraft.isFlattenableWrapper) {
      draft.children.addAll(childDraft.children);
    } else {
      draft.children.add(childDraft);
    }
  });

  return draft;
}

Future<String> _handleRequestData(String? message) async {
  if (message != _kGetRichTreeCommand) {
    // No es nuestro comando -- incluye tanto "no me llamaron a mí" (mensaje null/otro) como el
    // caso general de compatibilidad: devolvemos el mismo centinela que Flutter Driver usaría
    // sin ningún handler registrado, para no inventar un shape de error nuevo.
    return kFlutterMcpNoHandlerSentinel;
  }
  await _ensureSemanticsForInspection();
  final Element? root = WidgetsBinding.instance.rootElement;
  if (root == null) {
    return kFlutterMcpNoHandlerSentinel;
  }
  return jsonEncode(_describeElement(root).toJson());
}

/// Reemplazo drop-in de `enableFlutterDriverExtension()` que además registra el `DataHandler`
/// custom de flutter-native-mcp para exponer `key`/`tooltip`/`semanticsLabel`/`bounds` reales en
/// `flutter_snapshot` (Fase B). Llamalo en vez de `enableFlutterDriverExtension()` sin
/// argumentos -- todos los comandos estándar (tap, get_text, scroll, etc.) siguen funcionando
/// exactamente igual.
///
/// ⚠️ Si tu app YA pasa su propio `handler`/`finders`/`commands` a
/// `enableFlutterDriverExtension` para sus propios fines de testing, NO reemplaces esa llamada
/// por esta: perderías tu handler. flutter-native-mcp sigue funcionando igual (tap/scroll/
/// enter_text/etc.) sin esto -- simplemente sin key/tooltip/semantics/bounds reales en
/// `flutter_snapshot` (fallback automático, ver README, "Compatibilidad y riesgos").
void enableFlutterMcpDriverExtension() {
  enableFlutterDriverExtension(handler: _handleRequestData);
}
