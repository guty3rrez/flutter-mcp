use cucumber::{World, given, then, when};
use flutter_mcp::{ErrorDetector, FlutterError, TreePruner, WidgetNode};
use serde_json::{Value, json};

#[derive(Debug, Default, World)]
pub struct FlutterWorld {
    raw_json: Value,
    pruned_node: Option<WidgetNode>,
    captured_lines: Vec<String>,
    detected_errors: Vec<FlutterError>,
}

#[given(
    expr = "a Flutter diagnostics tree containing a Padding wrapper around an ElevatedButton with key {string} and text {string}"
)]
fn setup_flutter_tree(world: &mut FlutterWorld, key: String, text: String) {
    world.raw_json = json!({
        "description": "Padding",
        "children": [
            {
                "description": "ElevatedButton",
                "properties": [
                    {"name": "key", "description": key}
                ],
                "children": [
                    {
                        "description": "Text",
                        "properties": [
                            {"name": "data", "description": text}
                        ]
                    }
                ]
            }
        ]
    });
}

#[when("the tree pruner processes the raw diagnostics tree")]
fn process_tree(world: &mut FlutterWorld) {
    world.pruned_node = TreePruner::prune_diagnostics_tree(&world.raw_json);
}

#[then(expr = "the resulting node should have widget_type {string}")]
fn assert_widget_type(world: &mut FlutterWorld, expected_type: String) {
    let node = world.pruned_node.as_ref().expect("Pruned node must exist");
    assert_eq!(node.widget_type, expected_type);
}

#[then(expr = "the node should have key {string}")]
fn assert_key(world: &mut FlutterWorld, expected_key: String) {
    let node = world.pruned_node.as_ref().expect("Pruned node must exist");
    assert_eq!(node.key.as_deref(), Some(expected_key.as_str()));
}

#[then(expr = "the node should have text {string}")]
fn assert_text(world: &mut FlutterWorld, expected_text: String) {
    let node = world.pruned_node.as_ref().expect("Pruned node must exist");
    // El texto está en el hijo Text
    let child_text = node.children.first().and_then(|c| c.text.as_deref());
    assert_eq!(child_text, Some(expected_text.as_str()));
}

#[then("the node should be marked as interactive")]
fn assert_interactive(world: &mut FlutterWorld) {
    let node = world.pruned_node.as_ref().expect("Pruned node must exist");
    assert!(node.is_interactive);
}

#[given(expr = "a Flutter app printed a red screen exception block for {string}")]
fn setup_red_screen(world: &mut FlutterWorld, message: String) {
    world.captured_lines = vec![
        "══╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞══════════════════════".into(),
        "The following error was thrown building MyWidget:".into(),
        message,
        "═══════════════════════════════════════════════════════════════".into(),
    ];
}

#[given(expr = "a Dart app printed an unhandled exception with message {string}")]
fn setup_plain_unhandled(world: &mut FlutterWorld, message: String) {
    world.captured_lines = vec![
        "Unhandled exception:".into(),
        message,
        "#0      main.<anonymous closure> (file.dart:10:5)".into(),
        "#1      main (file.dart:20:3)".into(),
        // El detector cierra el bloque "PlainUnhandled" con la primera línea que ya no es
        // un frame de stack (no hay un separador explícito como en el red screen de Flutter);
        // esta línea final simula lo próximo que la VM imprime tras el stack trace.
        String::new(),
    ];
}

#[when("the error detector processes the captured output line by line")]
fn run_error_detector(world: &mut FlutterWorld) {
    let mut detector = ErrorDetector::new();
    world.detected_errors.clear();
    for line in &world.captured_lines {
        if let Some(error) = detector.process_line(line, 0) {
            world.detected_errors.push(error);
        }
    }
}

#[then(expr = "it should report exactly {int} detected error(s)")]
fn assert_detected_count(world: &mut FlutterWorld, count: usize) {
    assert_eq!(world.detected_errors.len(), count);
}

#[then(expr = "the detected error message should mention {string}")]
fn assert_message_mentions(world: &mut FlutterWorld, needle: String) {
    let error = world
        .detected_errors
        .last()
        .expect("Debe haber un error detectado");
    assert!(
        error.message.contains(&needle),
        "'{}' no contiene '{}'",
        error.message,
        needle
    );
}

#[then(expr = "the detected error stack trace should mention {string}")]
fn assert_stack_mentions(world: &mut FlutterWorld, needle: String) {
    let error = world
        .detected_errors
        .last()
        .expect("Debe haber un error detectado");
    let stack = error.stack_trace.as_deref().unwrap_or("");
    assert!(stack.contains(&needle), "'{stack}' no contiene '{needle}'");
}

#[tokio::main]
async fn main() {
    FlutterWorld::run("tests/features").await;
}
