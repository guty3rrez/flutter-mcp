use cucumber::{World, given, then, when};
use flutter_native_mcp::{TreePruner, WidgetNode};
use serde_json::{Value, json};

#[derive(Debug, Default, World)]
pub struct FlutterWorld {
    raw_json: Value,
    pruned_node: Option<WidgetNode>,
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

#[tokio::main]
async fn main() {
    FlutterWorld::run("tests/features").await;
}
