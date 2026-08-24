//! Convert ComfyUI frontend **UI** format to the API format expected by `/prompt`.
//!
//! UI format example (what `Save(...)` writes to disk):
//! ```json
//! {
//!   "id": "...", "revision": 0, "last_node_id": ..., "last_link_id": ...,
//!   "nodes": [
//!     {"id": 57, "type": "KSampler", "inputs": [
//!        {"name": "seed", "widget": {"name": "seed"}, "link": null},
//!        {"name": "model", "link": 5}
//!      ], "widgets_values": [12345, "euler", "normal", 1.0, ...]},
//!     ...
//!   ],
//!   "links": [[5, 42, 0, 57, 0, "MODEL"]],
//!   ...
//! }
//! ```
//!
//! API format (what ComfyUI's `/prompt` accepts):
//! ```json
//! { "57": { "class_type": "KSampler", "inputs": { "seed": 12345, "model": ["42", 0] } } }
//! ```
//!
//! Conversion rules:
//! 1. Each non-skipped node → API entry; key = node.id stringified.
//! 2. `class_type` = node.type (UUIDs are preserved verbatim).
//! 3. For each input: if `link` is non-null, value = `["<src_node_id>", src_slot]`.
//!    Otherwise it's a widget — pull from `widgets_values`.
//! 4. Reroute nodes are skipped (not output) but links through them are followed
//!    to the real source. Cycle-protected by a max-hop limit.
//! 5. MarkdownNote / Note / other annotation-only nodes (no inputs, no outputs)
//!    are dropped; their links are reported as errors rather than silently lost.
//! 6. widgets_values may be an array (zip in order with widget-marked inputs) or
//!    a JSON object (lookup by widget name). Array elements of shape `{"value": x}`
//!    are unwrapped to `x`.

use serde_json::{Map, Value};
use std::collections::HashMap;

const MAX_REROUTE_HOPS: usize = 32;

#[derive(Debug, Clone)]
struct UiNode {
    id: String,
    class_type: String,
    inputs: Vec<UiInput>,
    widgets_values: Option<Value>,
    is_reroute: bool,
}

#[derive(Debug, Clone)]
struct UiInput {
    name: String,
    link: Option<i64>,
    is_widget: bool,
}

#[derive(Debug, Clone)]
struct Link {
    id: i64,
    src_node: String,
    src_slot: i64,
}

/// True if the JSON value looks like ComfyUI frontend UI format
/// (a top-level object with a non-empty `nodes` array).
pub fn is_ui_format(value: &Value) -> bool {
    value
        .get("nodes")
        .and_then(|n| n.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

/// Convert UI format JSON to API format JSON.
///
/// Returns an error when:
/// - The JSON doesn't have a parseable `nodes` array.
/// - A link points to a node that was dropped (annotation-only) and cannot be
///   resolved through a Reroute.
pub fn ui_to_api(ui: &Value) -> Result<Value, String> {
    let nodes_arr = ui
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| "missing `nodes` array — not a UI-format workflow".to_string())?;
    let links_arr = ui
        .get("links")
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();

    // ---- Parse links: [link_id, src_node, src_slot, dst_node, dst_slot, type]
    let mut by_id: HashMap<i64, Link> = HashMap::new();
    let mut dst_of: HashMap<(String, i64), i64> = HashMap::new(); // (dst_node, dst_slot) -> link_id
    for entry in &links_arr {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 5 => a,
            _ => continue,
        };
        let id = match arr.first().and_then(|x| x.as_i64()) {
            Some(v) => v,
            None => continue,
        };
        let src_node = arr
            .get(1)
            .and_then(|x| x.as_i64())
            .map(|n| n.to_string())
            .or_else(|| arr.get(1).and_then(|x| x.as_str()).map(String::from))
            .unwrap_or_default();
        let src_slot = arr.get(2).and_then(|x| x.as_i64()).unwrap_or(0);
        let dst_node = arr
            .get(3)
            .and_then(|x| x.as_i64())
            .map(|n| n.to_string())
            .or_else(|| arr.get(3).and_then(|x| x.as_str()).map(String::from))
            .unwrap_or_default();
        let dst_slot = arr.get(4).and_then(|x| x.as_i64()).unwrap_or(0);
        by_id.insert(
            id,
            Link {
                id,
                src_node,
                src_slot,
            },
        );
        dst_of.insert((dst_node, dst_slot), id);
    }

    // ---- Parse nodes
    let mut nodes: Vec<UiNode> = Vec::with_capacity(nodes_arr.len());
    for raw in nodes_arr {
        let id = match raw.get("id") {
            Some(Value::Number(n)) => match n.as_i64() {
                Some(i) => i.to_string(),
                None => continue,
            },
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let class_type = raw
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let mut inputs: Vec<UiInput> = Vec::new();
        if let Some(arr) = raw.get("inputs").and_then(|i| i.as_array()) {
            for inp in arr {
                let name = inp
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                // Allow empty-name inputs (Reroute nodes use them) — they'll
                // only be used for upstream-link traversal, not emitted.
                let link = inp.get("link").and_then(|l| l.as_i64());
                let is_widget = inp.get("widget").is_some();
                inputs.push(UiInput {
                    name,
                    link,
                    is_widget,
                });
            }
        }
        let widgets_values = raw.get("widgets_values").cloned();
        let is_reroute = class_type == "Reroute";
        nodes.push(UiNode {
            id,
            class_type,
            inputs,
            widgets_values,
            is_reroute,
        });
    }

    // ---- Walk widgets_values for each node, building widget-value lookup.
    // widget index map (input name -> value)
    let mut widget_values_by_node: HashMap<String, HashMap<String, Value>> = HashMap::new();
    let mut widget_arrays_by_node: HashMap<String, Vec<Value>> = HashMap::new();

    for node in &nodes {
        if node.is_reroute {
            continue;
        }
        let Some(wv) = &node.widgets_values else {
            continue;
        };
        match wv {
            Value::Object(map) => {
                let mut inner = HashMap::new();
                for (k, v) in map {
                    inner.insert(k.clone(), unwrap_value_wrapper(v));
                }
                widget_values_by_node.insert(node.id.clone(), inner);
            }
            Value::Array(arr) => {
                widget_arrays_by_node.insert(node.id.clone(), arr.iter().map(unwrap_value_wrapper).collect());
            }
            _ => {}
        }
    }

    // ---- Build output map.
    // For each non-skipped node, build an API entry. For each input, resolve
    // the value.
    let mut out: Map<String, Value> = Map::new();

    // Classify nodes: output vs skipped
    for node in &nodes {
        if node.is_reroute {
            continue;
        }
        if node.class_type.is_empty() {
            continue;
        }
        // Note / MarkdownNote have neither inputs nor outputs -> pure annotation
        let has_link_input = node.inputs.iter().any(|i| i.link.is_some());
        let has_named_widget_input = node.inputs.iter().any(|i| !i.name.is_empty());
        let is_annotation_only =
            !has_link_input && !has_named_widget_input;
        if is_annotation_only
            && matches!(node.class_type.as_str(), "Note" | "MarkdownNote")
        {
            continue;
        }

        let mut inputs_map: Map<String, Value> = Map::new();
        let wv_array = widget_arrays_by_node.get(&node.id);
        let wv_obj = widget_values_by_node.get(&node.id);

        // Walk widget-marked inputs in declaration order to consume wv_array by index
        let mut widget_idx: usize = 0;
        for input in &node.inputs {
            // Skip unnamed inputs (they only matter for Reroute traversal).
            if input.name.is_empty() {
                continue;
            }
            if let Some(link_id) = input.link {
                // Resolve through Reroutes
                match resolve_link(link_id, &by_id, &nodes) {
                    Some((src_node, src_slot)) => {
                        inputs_map.insert(
                            input.name.clone(),
                            Value::Array(vec![
                                Value::String(src_node),
                                Value::Number(serde_json::Number::from(src_slot)),
                            ]),
                        );
                    }
                    None => {
                        return Err(format!(
                            "node `{}` input `{}` (link {}) cannot be resolved — source is a dropped annotation or unreachable Reroute",
                            node.id, input.name, link_id
                        ));
                    }
                }
                continue;
            }
            if !input.is_widget {
                // No link, not a widget — skip (typically localized display fields).
                continue;
            }
            // Widget value lookup
            let val = if let Some(arr) = wv_array {
                arr.get(widget_idx).cloned().unwrap_or(Value::Null)
            } else if let Some(map) = wv_obj {
                map.get(&input.name).cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            // Only insert if the value is meaningful — ComfyUI doesn't like
            // explicit nulls where it expects a default.
            if !val.is_null() {
                inputs_map.insert(input.name.clone(), val);
            }
            widget_idx += 1;
        }

        let entry = serde_json::json!({
            "class_type": node.class_type,
            "inputs": inputs_map,
        });
        out.insert(node.id.clone(), entry);
    }

    Ok(Value::Object(out))
}

/// Follow a link chain through any number of Reroute nodes until a real source
/// is found. Returns (src_node_id, src_slot).
fn resolve_link(
    mut link_id: i64,
    by_id: &HashMap<i64, Link>,
    nodes: &[UiNode],
) -> Option<(String, i64)> {
    // node index
    let node_index: HashMap<&str, &UiNode> =
        nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for _ in 0..MAX_REROUTE_HOPS {
        let link = by_id.get(&link_id)?.clone();
        let src = node_index.get(link.src_node.as_str())?;
        if src.is_reroute {
            // Reroute: its inputs[*].link should point to the upstream
            let upstream = src.inputs.iter().find_map(|i| i.link);
            match upstream {
                Some(next) => link_id = next,
                None => return None,
            }
            continue;
        }
        return Some((link.src_node, link.src_slot));
    }
    None
}

/// ComfyUI sometimes wraps widget entries in `{"value": x}` (combo / proxy
/// widgets). Strip the wrapper.
fn unwrap_value_wrapper(v: &Value) -> Value {
    if let Value::Object(m) = v {
        if m.len() == 1 {
            if let Some(inner) = m.get("value") {
                return inner.clone();
            }
        }
    }
    v.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load_ui(path: &str) -> Option<Value> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn must_exist(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }

    fn assert_link_resolves(api: &Value, key: &str, input: &str) {
        let entry = api.get(key).unwrap_or_else(|| panic!("missing node {key}"));
        let inputs = entry.get("inputs").unwrap();
        let v = inputs
            .get(input)
            .unwrap_or_else(|| panic!("node {key} missing input {input}"));
        assert!(
            v.is_array() && v.as_array().unwrap().len() == 2,
            "expected link array for {key}.{input}, got {v}"
        );
    }

    fn assert_literal(api: &Value, key: &str, input: &str, expected_predicate: fn(&Value) -> bool) {
        let entry = api.get(key).expect("missing node");
        let v = entry
            .get("inputs")
            .and_then(|i| i.get(input))
            .unwrap_or_else(|| panic!("node {key} missing input {input}"));
        assert!(
            !v.is_array(),
            "expected literal for {key}.{input}, got array {v}"
        );
        assert!(
            expected_predicate(v),
            "value for {key}.{input} did not match predicate: {v}"
        );
    }

    #[test]
    fn ui_to_api_basic_text_link() {
        let ui = json!({
            "nodes": [
                {
                    "id": 10, "type": "CLIPTextEncode",
                    "inputs": [
                        {"name": "text", "widget": {"name": "text"}, "link": null}
                    ],
                    "widgets_values": ["a cat"]
                },
                {
                    "id": 20, "type": "KSampler",
                    "inputs": [
                        {"name": "positive", "link": 7, "type": "CONDITIONING"},
                        {"name": "seed", "widget": {"name": "seed"}, "link": null},
                        {"name": "steps", "widget": {"name": "steps"}, "link": null}
                    ],
                    "widgets_values": [42, 20]
                }
            ],
            "links": [[7, 10, 0, 20, 0, "CONDITIONING"]]
        });
        let api = ui_to_api(&ui).unwrap();
        // CLIPTextEncode.text is a string literal
        assert_literal(&api, "10", "text", |v| v == "a cat");
        // KSampler.seed/steps are numbers
        assert_literal(&api, "20", "seed", |v| v.as_i64() == Some(42));
        assert_literal(&api, "20", "steps", |v| v.as_i64() == Some(20));
        // positive is a link array
        assert_link_resolves(&api, "20", "positive");
        assert_eq!(
            api["20"]["inputs"]["positive"],
            json!(["10", 0])
        );
    }

    #[test]
    fn ui_to_api_drops_markdown_note() {
        let ui = json!({
            "nodes": [
                {"id": 1, "type": "MarkdownNote", "inputs": [], "widgets_values": ["hello"]},
                {"id": 2, "type": "SaveImage", "inputs": [{"name": "images", "link": null}], "widgets_values": []}
            ],
            "links": []
        });
        let api = ui_to_api(&ui).unwrap();
        assert!(api.get("1").is_none());
        assert!(api.get("2").is_some());
    }

    #[test]
    fn ui_to_api_skips_reroute_and_follows_link() {
        let ui = json!({
            "nodes": [
                {"id": 1, "type": "Source", "inputs": [], "widgets_values": []},
                {
                    "id": 2, "type": "Reroute",
                    "inputs": [{"name": "", "link": 100, "type": "*"}],
                    "widgets_values": []
                },
                {
                    "id": 3, "type": "Sink",
                    "inputs": [{"name": "in", "link": 200, "type": "*"}],
                    "widgets_values": []
                }
            ],
            "links": [
                [100, 1, 0, 2, 0, "*"],
                [200, 2, 0, 3, 0, "*"]
            ]
        });
        let api = ui_to_api(&ui).unwrap();
        assert!(api.get("2").is_none(), "reroute should be dropped");
        // Sink.in should resolve to Source via the reroute
        let link = &api["3"]["inputs"]["in"];
        assert_eq!(link, &json!(["1", 0]));
    }

    #[test]
    fn ui_to_api_handles_value_wrapper_in_widgets() {
        let ui = json!({
            "nodes": [
                {
                    "id": 1, "type": "Combo",
                    "inputs": [{"name": "choice", "widget": {"name": "choice"}, "link": null}],
                    "widgets_values": [{"value": "euler"}]
                }
            ],
            "links": []
        });
        let api = ui_to_api(&ui).unwrap();
        assert_eq!(api["1"]["inputs"]["choice"], json!("euler"));
    }

    #[test]
    fn ui_to_api_detects_format() {
        assert!(is_ui_format(&json!({"nodes": [{}], "links": []})));
        assert!(!is_ui_format(&json!({"57": {"class_type": "KSampler"}})));
        assert!(!is_ui_format(&json!({"nodes": []})));
    }

    #[test]
    fn convert_real_z_image_turbo() {
        let path = "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows/z_image_turbo.json";
        if !must_exist(path) {
            eprintln!("skip: {path} not present");
            return;
        }
        let ui = load_ui(path).expect("read z_image_turbo");
        let api = ui_to_api(&ui).expect("convert z_image_turbo");
        // Node 57 has many widgets including text/width/height/seed/steps.
        assert!(api.get("57").is_some(), "node 57 should be present");
        assert!(
            api["57"]["inputs"].get("text").is_some(),
            "text widget should be present on 57"
        );
        // text should be a string literal (the prompt)
        let txt = &api["57"]["inputs"]["text"];
        assert!(txt.is_string(), "text should be string literal, got {txt}");
        // width/height/seed/steps are numbers
        for k in ["width", "height", "seed", "steps"] {
            let v = &api["57"]["inputs"][k];
            assert!(v.is_number(), "{k} should be number, got {v}");
        }
        // SaveImage node 9: its images input links from 57 (link 62)
        assert_link_resolves(&api, "9", "images");
        assert_eq!(api["9"]["inputs"]["images"], json!(["57", 0]));
    }

    #[test]
    fn convert_real_image_krea2_turbo_t2i() {
        let path = "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows/image_krea2_turbo_t2i.json";
        if !must_exist(path) {
            eprintln!("skip: {path} not present");
            return;
        }
        let ui = load_ui(path).expect("read krea2");
        let api = ui_to_api(&ui).expect("convert krea2");
        // Two MarkdownNote nodes should be dropped.
        assert!(api.get("47").is_none());
        assert!(api.get("50").is_none());
        // Node 29 (SaveImage) — images input links from node 30
        assert_link_resolves(&api, "29", "images");
        // Node 30 — a UUID-typed node; class_type should be preserved verbatim
        let node30 = api.get("30").expect("node 30 present");
        assert_eq!(
            node30["class_type"], "b0e5ca93-2731-42b9-8e0a-d28ea851ff81"
        );
        // width_1 / height_1 of node 30 are links from node 49 (ResolutionSelector)
        assert_link_resolves(&api, "30", "width_1");
        assert_link_resolves(&api, "30", "height_1");
        assert_eq!(api["30"]["inputs"]["width_1"], json!(["49", 0]));
        assert_eq!(api["30"]["inputs"]["height_1"], json!(["49", 1]));
    }

    #[test]
    fn convert_real_birefnet_remove_background() {
        let path = "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows/birefnet_remove_background.json";
        if !must_exist(path) {
            eprintln!("skip: {path} not present");
            return;
        }
        let ui = load_ui(path).expect("read birefnet");
        let api = ui_to_api(&ui).expect("convert birefnet");
        // LoadImage (17) image input is a widget; the widget_values index 0 is the filename
        assert!(api.get("17").is_some());
        let img = &api["17"]["inputs"]["image"];
        assert!(img.is_string(), "image should be a literal string, got {img}");
        // Node 19 (UUID type) gets image from LoadImage via link 15
        assert_link_resolves(&api, "19", "image");
        assert_eq!(api["19"]["inputs"]["image"], json!(["17", 0]));
    }

    #[test]
    fn convert_real_zimage() {
        let path = "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows/zimage.json";
        if !must_exist(path) {
            eprintln!("skip: {path} not present");
            return;
        }
        let ui = load_ui(path).expect("read zimage");
        let api = ui_to_api(&ui).expect("convert zimage");
        // Node 76 is the UUID-typed combined sampler; class_type preserved
        assert_eq!(
            api["76"]["class_type"], "9b9009e4-2d3d-445f-9be5-6063f465757e"
        );
        // text + text_1 should be strings
        assert!(api["76"]["inputs"]["text"].is_string());
        assert!(api["76"]["inputs"]["text_1"].is_string());
        // width_1 / height_1 are links from node 94 (ResolutionSelector)
        assert_link_resolves(&api, "76", "width_1");
        assert_link_resolves(&api, "76", "height_1");
        assert_eq!(api["76"]["inputs"]["width_1"], json!(["94", 0]));
        assert_eq!(api["76"]["inputs"]["height_1"], json!(["94", 1]));
        // MarkdownNote (86) dropped
        assert!(api.get("86").is_none());
    }

    #[test]
    fn convert_reports_unresolvable_link() {
        // A link whose source node is missing entirely.
        let ui = json!({
            "nodes": [
                {
                    "id": 1, "type": "Sink",
                    "inputs": [{"name": "x", "link": 99, "type": "*"}],
                    "widgets_values": []
                }
            ],
            "links": [[99, 999, 0, 1, 0, "*"]]
        });
        let res = ui_to_api(&ui);
        assert!(res.is_err(), "expected error, got Ok");
        assert!(res.unwrap_err().contains("cannot be resolved"));
    }
}