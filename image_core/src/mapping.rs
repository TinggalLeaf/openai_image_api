//! Workflow parameter mapping.
//!
//! - `read_field` / `write_field`: dotted-path JSON access ("nodeId.inputs.field")
//! - `detect_mapping`: best-effort reverse-engineer of an unknown workflow.

use crate::models::ParamMapping;
use serde_json::Value;

/// Read a value at the dotted path. Path format: "<node_id>.inputs.<field>".
/// Returns None if any segment is missing.
pub fn read_field<'a>(workflow: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    let mut segs = path.split('.');
    let first = segs.next()?;
    let mut cur = workflow.get(first)?;
    for s in segs {
        cur = cur.get(s)?;
    }
    Some(cur)
}

/// Write a value at the dotted path, mutating the JSON in place.
/// Returns true if the path existed and was written.
pub fn write_field(workflow: &mut Value, path: &str, value: Value) -> bool {
    if path.is_empty() {
        return false;
    }
    let mut segs: Vec<&str> = path.split('.').collect();
    let leaf = match segs.pop() {
        Some(l) => l,
        None => return false,
    };
    let mut cur = workflow;
    for s in segs {
        cur = match cur.get_mut(s) {
            Some(v) => v,
            None => return false,
        };
    }
    if let Some(slot) = cur.get_mut(leaf) {
        *slot = value;
        true
    } else {
        false
    }
}

/// True when the workflow value at `path` is a numeric literal (not a link array).
fn is_numeric_literal(v: &Value) -> bool {
    v.is_number()
}

/// Walk a link ["node_id", output_index] → look up the source node's input field by name.
fn resolve_link_source(workflow: &Value, link: &Value, field: &str) -> Option<String> {
    let arr = link.as_array()?;
    let src_id = arr.first()?.as_str()?;
    let node = workflow.get(src_id)?;
    let inputs = node.get("inputs")?;
    let val = inputs.get(field)?;
    if val.is_string() || val.is_number() || val.is_boolean() {
        Some(format!("{}.inputs.{}", src_id, field))
    } else {
        None
    }
}

/// Detect a sampler (KSampler family) node id from a workflow.
fn find_sampler_node(workflow: &Value) -> Option<String> {
    let obj = workflow.as_object()?;
    for (id, node) in obj {
        let class = node.get("class_type")?.as_str()?;
        let inputs = node.get("inputs")?;
        let is_sampler = class.contains("KSampler")
            || (class.contains("Sampler") && inputs.get("seed").is_some() && inputs.get("steps").is_some());
        if is_sampler {
            return Some(id.clone());
        }
    }
    None
}

/// Find a CLIPTextEncode reached from the sampler's positive/negative input.
fn find_clip_for_link(workflow: &Value, link: &Value) -> Option<String> {
    let arr = link.as_array()?;
    let src_id = arr.first()?.as_str()?;
    let node = workflow.get(src_id)?;
    let class = node.get("class_type")?.as_str()?;
    if class == "CLIPTextEncode" {
        Some(src_id.to_string())
    } else {
        None
    }
}

/// Find an Empty...LatentImage node that owns width/height/batch_size as numeric literals.
fn find_empty_latent(workflow: &Value) -> Option<String> {
    let obj = workflow.as_object()?;
    for (id, node) in obj {
        let class = node.get("class_type")?.as_str()?;
        if class.starts_with("Empty") && class.contains("Latent") {
            let inputs = node.get("inputs")?;
            // Prefer the node where width/height/batch_size are direct numbers.
            let w = inputs.get("width");
            let h = inputs.get("height");
            let has_numeric = [w, h].iter().any(|f| f.map(is_numeric_literal).unwrap_or(false));
            if has_numeric {
                return Some(id.clone());
            }
        }
    }
    // Fallback: any Empty*Latent* node
    for (id, node) in obj {
        let class = node.get("class_type")?.as_str()?;
        if class.starts_with("Empty") && class.contains("Latent") {
            return Some(id.clone());
        }
    }
    None
}

/// Auto-detect parameter mapping for a ComfyUI workflow JSON.
///
/// Strategy:
/// 1. Find a KSampler-family node. Its inputs seed/steps/cfg/sampler_name/scheduler
///    map directly (if the input exists as a numeric literal or string).
/// 2. From sampler.positive link → CLIPTextEncode.text → prompt.
/// 3. From sampler.negative link → CLIPTextEncode.text → negative_prompt.
///    Anything else (ConditioningZeroOut, etc.) leaves negative_prompt empty.
/// 4. Find an Empty*Latent* node. width/height/batch_size are only mapped when
///    they're numeric literals (not link arrays). If they ARE links, we try to
///    trace through one level to find a numeric/string source — if unreachable,
///    we leave the mapping empty (admin can fill it in manually).
pub fn detect_mapping(workflow: &Value) -> ParamMapping {
    let mut m = ParamMapping::default();

    // LoadImage is detected up-front so even samplre-less workflows (e.g.
    // pure upscale / edit / remove-background flows) get the image mapping.
    if let Some(load_id) = find_load_image(workflow) {
        m.image = format!("{}.inputs.image", load_id);
    }

    // Flat-field fallback runs early so merged-node workflows (single UUID
    // mega-node with no classic KSampler) get their prompt/seed/etc. mapped.
    fill_flat_field_fallback(workflow, &mut m);

    let sampler_id = match find_sampler_node(workflow) {
        Some(id) => id,
        None => return m,
    };

    let sampler_inputs = match workflow.get(&sampler_id).and_then(|n| n.get("inputs")) {
        Some(i) => i,
        None => return m,
    };

    // Sampler params: only map if the field is present (don't enforce).
    if sampler_inputs.get("seed").is_some() {
        m.seed = format!("{}.inputs.seed", sampler_id);
    }
    if sampler_inputs.get("steps").is_some() {
        m.steps = format!("{}.inputs.steps", sampler_id);
    }
    if sampler_inputs.get("cfg").is_some() {
        m.cfg = format!("{}.inputs.cfg", sampler_id);
    }
    if sampler_inputs.get("sampler_name").is_some() {
        m.sampler = format!("{}.inputs.sampler_name", sampler_id);
    }
    if sampler_inputs.get("scheduler").is_some() {
        m.scheduler = format!("{}.inputs.scheduler", sampler_id);
    }

    // Positive → prompt
    if let Some(pos_link) = sampler_inputs.get("positive") {
        if let Some(clip_id) = find_clip_for_link(workflow, pos_link) {
            let clip_inputs = workflow.get(&clip_id).and_then(|n| n.get("inputs"));
            if let Some(ci) = clip_inputs {
                if let Some(text_val) = ci.get("text") {
                    if text_val.is_string() {
                        m.prompt = format!("{}.inputs.text", clip_id);
                    } else if text_val.is_array() {
                        // Text is itself a link — try to walk one hop to a
                        // node whose text input is a string. If that fails,
                        // leave prompt empty (admin can fill it manually).
                        // We DON'T loop forever to avoid hangs on cycles.
                        if let Some(src) = resolve_link_source(workflow, text_val, "text") {
                            if let Some(src_node) = workflow.get(src.split('.').next().unwrap_or("")) {
                                if let Some(src_inputs) = src_node.get("inputs") {
                                    if let Some(t) = src_inputs.get("text") {
                                        if t.is_string() {
                                            m.prompt = src;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Negative → negative_prompt
    if let Some(neg_link) = sampler_inputs.get("negative") {
        if let Some(clip_id) = find_clip_for_link(workflow, neg_link) {
            let clip_inputs = workflow.get(&clip_id).and_then(|n| n.get("inputs"));
            if let Some(ci) = clip_inputs {
                if let Some(text_val) = ci.get("text") {
                    if text_val.is_string() {
                        m.negative_prompt = format!("{}.inputs.text", clip_id);
                    }
                }
            }
        }
    }

    // Latent image params
    if let Some(latent_id) = find_empty_latent(workflow) {
        let latent_inputs = workflow
            .get(&latent_id)
            .and_then(|n| n.get("inputs"));
        if let Some(li) = latent_inputs {
            if let Some(w) = li.get("width") {
                if is_numeric_literal(w) {
                    m.width = format!("{}.inputs.width", latent_id);
                } else if w.is_array() {
                    // Width is a link — try one hop back to a numeric source.
                    if let Some(src) = resolve_link_source(workflow, w, "value") {
                        if let Some(src_node) = workflow.get(src.split('.').next().unwrap_or("")) {
                            if let Some(si) = src_node.get("inputs") {
                                if let Some(v) = si.get("value") {
                                    if v.is_number() {
                                        m.width = src;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(h) = li.get("height") {
                if is_numeric_literal(h) {
                    m.height = format!("{}.inputs.height", latent_id);
                } else if h.is_array() {
                    if let Some(src) = resolve_link_source(workflow, h, "value") {
                        if let Some(src_node) = workflow.get(src.split('.').next().unwrap_or("")) {
                            if let Some(si) = src_node.get("inputs") {
                                if let Some(v) = si.get("value") {
                                    if v.is_number() {
                                        m.height = src;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(b) = li.get("batch_size") {
                if is_numeric_literal(b) {
                    m.batch_size = format!("{}.inputs.batch_size", latent_id);
                }
            }
        }
    }

    m
}

/// Flat-field fallback: walk every node, score each by the number of flat
/// fields it owns (text/seed/steps/cfg/sampler_name/scheduler/width/height/
/// batch_size/positive/negative/…),), then write the path for any mapping
/// field that is still empty.
fn fill_flat_field_fallback(workflow: &Value, m: &mut ParamMapping) {
    // Each entry: (mapping_field_name, accepted_input_names, expected_kind)
    let specs: &[(&str, &[&str], FieldKind)] = &[
        ("prompt", &["text", "prompt", "positive", "positive_prompt"], FieldKind::String),
        ("negative_prompt", &["negative", "negative_prompt"], FieldKind::String),
        ("seed", &["seed"], FieldKind::Number),
        ("steps", &["steps"], FieldKind::Number),
        ("cfg", &["cfg"], FieldKind::Number),
        ("sampler", &["sampler_name", "sampler"], FieldKind::String),
        ("scheduler", &["scheduler"], FieldKind::String),
        ("width", &["width"], FieldKind::Number),
        ("height", &["height"], FieldKind::Number),
        ("batch_size", &["batch_size"], FieldKind::Number),
    ];

    // Per-node score: how many specs match in this node.
    let obj = match workflow.as_object() {
        Some(o) => o,
        None => return,
    };

    let mut node_scores: Vec<(&str, usize)> = Vec::with_capacity(obj.len());
    for (id, node) in obj {
        let inputs = match node.get("inputs").and_then(|i| i.as_object()) {
            Some(i) => i,
            None => continue,
        };
        let mut score = 0usize;
        for (_, names, kind) in specs {
            for n in *names {
                if let Some(v) = inputs.get(*n) {
                    if !v.is_array() && matches_kind(v, *kind) {
                        score += 1;
                        break; // one hit per spec is enough to count the node
                    }
                }
            }
        }
        if score > 0 {
            node_scores.push((id.as_str(), score));
        }
    }
    if node_scores.is_empty() {
        return;
    }
    // Sort: higher score first; ties broken by first-seen order (Vec order is insertion order).
    node_scores.sort_by(|a, b| b.1.cmp(&a.1));

    let winner_id = node_scores[0].0;
    let winner_inputs = match workflow.get(winner_id).and_then(|n| n.get("inputs")).and_then(|i| i.as_object()) {
        Some(i) => i,
        None => return,
    };

    // Field-by-field assignment, only overwriting empty entries.
    for (field, names, kind) in specs {
        if !mapping_field(m, field).is_empty() {
            continue; // already filled by standard detection
        }
        for input_name in *names {
            if let Some(v) = winner_inputs.get(*input_name) {
                if !v.is_array() && matches_kind(v, *kind) {
                    let path = format!("{}.inputs.{}", winner_id, input_name);
                    set_mapping_field(m, field, path);
                    break;
                }
            }
        }
    }
}

fn mapping_field<'a>(m: &'a ParamMapping, field: &str) -> &'a str {
    match field {
        "prompt" => &m.prompt,
        "negative_prompt" => &m.negative_prompt,
        "seed" => &m.seed,
        "steps" => &m.steps,
        "cfg" => &m.cfg,
        "sampler" => &m.sampler,
        "scheduler" => &m.scheduler,
        "width" => &m.width,
        "height" => &m.height,
        "batch_size" => &m.batch_size,
        _ => "",
    }
}

fn set_mapping_field(m: &mut ParamMapping, field: &str, path: String) {
    match field {
        "prompt" => m.prompt = path,
        "negative_prompt" => m.negative_prompt = path,
        "seed" => m.seed = path,
        "steps" => m.steps = path,
        "cfg" => m.cfg = path,
        "sampler" => m.sampler = path,
        "scheduler" => m.scheduler = path,
        "width" => m.width = path,
        "height" => m.height = path,
        "batch_size" => m.batch_size = path,
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum FieldKind {
    String,
    Number,
}

fn matches_kind(v: &Value, k: FieldKind) -> bool {
    match k {
        FieldKind::String => v.is_string(),
        FieldKind::Number => v.is_number(),
    }
}

/// Find a LoadImage (or LoadImageMask) node — these accept user-uploaded
/// image inputs and are the wiring point for /v1/images/edits.
fn find_load_image(workflow: &Value) -> Option<String> {
    let obj = workflow.as_object()?;
    for (id, node) in obj {
        let class = node.get("class_type")?.as_str()?;
        if class == "LoadImage" || class == "LoadImageMask" {
            return Some(id.clone());
        }
    }
    None
}

/// Classify the workflow into one of:
/// - "audio": contains audio save/output nodes
/// - "video": contains video save/output nodes
/// - "vision": contains image-understanding / captioning nodes
/// - "i2i":   contains LoadImage/LoadImageMask AND an image output
/// - "t2i":   contains SaveImage / PreviewImage / SaveImageWebp / etc.
/// - "other": nothing image-like detected
///
/// Priority: audio > video > vision > i2i > t2i > other. This keeps an
/// audio workflow from being mis-tagged as "video" if it also happens to
/// contain a generic image preview node.
pub fn detect_kind(workflow: &Value) -> &'static str {
    let mut has_audio = false;
    let mut has_video = false;
    let mut has_vision = false;
    let mut has_i2i_input = false;
    let mut has_image_output = false;

    let obj = match workflow.as_object() {
        Some(o) => o,
        None => return "other",
    };

    for (_id, node) in obj {
        let class = node.get("class_type").and_then(|v| v.as_str()).unwrap_or("");
        let class_lower = class.to_ascii_lowercase();

        if class == "LoadImage" || class == "LoadImageMask" || class == "VHS_LoadVideo" {
            has_i2i_input = true;
        }
        if class == "SaveImage"
            || class == "PreviewImage"
            || class == "SaveImageWebp"
            || class_lower.contains("saveimage")
        {
            has_image_output = true;
        }

        // Audio: exact class match for common audio nodes, OR case-insensitive
        // token match for the "audio" / "tts" / "mms" tokens (avoid matching
        // e.g. "AudioDIT" alongside TTS to false-positive).
        if class == "SaveAudio"
            || class == "PreviewAudio"
            || class_lower.starts_with("saveaudio")
            || contains_token(&class_lower, "audio")
            || contains_token(&class_lower, "tts")
            || contains_token(&class_lower, "mms")
            || contains_token(&class_lower, "timbres")
        {
            has_audio = true;
        }
        if class == "SaveVideo"
            || class == "SaveWEBM"
            || class == "SaveAnimatedPNG"
            || class_lower.starts_with("savevideo")
            || contains_token(&class_lower, "video")
            || contains_token(&class_lower, "animatediff")
            || contains_token(&class_lower, "hunyuan_video")
            || contains_token(&class_lower, "wanvideo")
            || class_lower.contains("webm")
        {
            has_video = true;
        }
        if class == "Florence2Run"
            || class == "Florence2"
            || class == "LocateAnything"
            || class_lower.starts_with("florence")
            || contains_token(&class_lower, "locate")
            || contains_token(&class_lower, "caption")
        {
            has_vision = true;
        }
    }

    if has_audio {
        "audio"
    } else if has_video {
        "video"
    } else if has_vision {
        "vision"
    } else if has_i2i_input && has_image_output {
        "i2i"
    } else if has_image_output {
        "t2i"
    } else {
        "other"
    }
}

/// Case-insensitive token check: returns true if `haystack` contains `needle`
/// bounded by non-alphanumeric characters (or string edges). Prevents the
/// substring "wan" from matching inside "previewany".
fn contains_token(haystack: &str, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    let bytes = haystack.as_bytes();
    let n = needle.len();
    if n == 0 || n > bytes.len() {
        return false;
    }
    let mut i = 0;
    while i + n <= bytes.len() {
        if &bytes[i..i + n].to_ascii_lowercase() == needle.as_bytes() {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + n == bytes.len() || !bytes[i + n].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load(path: &str) -> Value {
        let text = std::fs::read_to_string(path).expect("read workflow");
        serde_json::from_str(&text).expect("parse workflow")
    }

    #[test]
    fn write_field_replaces_value() {
        let mut v = json!({"a": {"inputs": {"x": 1}}});
        assert!(write_field(&mut v, "a.inputs.x", json!(42)));
        assert_eq!(v["a"]["inputs"]["x"], json!(42));
    }

    #[test]
    fn write_field_empty_path_noop() {
        let mut v = json!({"a": 1});
        assert!(!write_field(&mut v, "", json!(2)));
    }

    #[test]
    fn detect_kind_audio_beats_video() {
        let wf = json!({
            "1": {"class_type": "SaveAudio"},
            "2": {"class_type": "PreviewImage"}
        });
        assert_eq!(detect_kind(&wf), "audio");
    }

    #[test]
    fn detect_kind_video() {
        let wf = json!({
            "1": {"class_type": "SaveVideo"},
            "2": {"class_type": "PreviewImage"}
        });
        assert_eq!(detect_kind(&wf), "video");
    }

    #[test]
    fn detect_kind_vision() {
        let wf = json!({
            "1": {"class_type": "Florence2Run"},
        });
        assert_eq!(detect_kind(&wf), "vision");
    }

    #[test]
    fn detect_kind_i2i_vs_t2i() {
        let t2i = json!({
            "1": {"class_type": "KSampler"},
            "2": {"class_type": "SaveImage"},
        });
        assert_eq!(detect_kind(&t2i), "t2i");

        let i2i = json!({
            "1": {"class_type": "LoadImage"},
            "2": {"class_type": "KSampler"},
            "3": {"class_type": "SaveImage"},
        });
        assert_eq!(detect_kind(&i2i), "i2i");
    }

    #[test]
    fn detect_kind_other() {
        let wf = json!({"1": {"class_type": "Note"}});
        assert_eq!(detect_kind(&wf), "other");
    }

    #[test]
    fn detect_mapping_picks_load_image() {
        let wf = json!({
            "1": {"class_type": "LoadImage", "inputs": {"image": "x.png"}},
            "2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0]}},
        });
        let m = detect_mapping(&wf);
        assert_eq!(m.image, "1.inputs.image");
    }

    #[test]
    fn detect_mapping_flat_field_fallback_merged_node() {
        // Synthetic "merged" mega-node workflow — KSampler/CLIP/EmptyLatent
        // are all flattened into a single UUID-class node.
        let wf = json!({
            "57": {
                "class_type": "f2fdebf6-dfaf-43b6-9eb2-7f70613cfdc1",
                "inputs": {
                    "text": "a cat sitting on a mat",
                    "width": 1024,
                    "height": 1024,
                    "seed": 12345,
                    "steps": 8,
                    "cfg": 1.0,
                    "sampler_name": "euler",
                    "scheduler": "normal",
                }
            },
            "9": {"class_type": "SaveImage", "inputs": {"images": ["57", 0]}},
        });
        let m = detect_mapping(&wf);
        assert_eq!(m.prompt, "57.inputs.text");
        assert_eq!(m.width, "57.inputs.width");
        assert_eq!(m.height, "57.inputs.height");
        assert_eq!(m.seed, "57.inputs.seed");
        assert_eq!(m.steps, "57.inputs.steps");
        assert_eq!(m.cfg, "57.inputs.cfg");
        assert_eq!(m.sampler, "57.inputs.sampler_name");
        assert_eq!(m.scheduler, "57.inputs.scheduler");
    }

    #[test]
    fn detect_mapping_real_z_image_turbo_after_ui_conversion() {
        // Convert the UI-format workflow on disk, then detect mapping and
        // assert the flat-field fallback picked up the merged-node fields.
        let path = "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows/z_image_turbo.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("skip: {path} not present");
            return;
        }
        let text = std::fs::read_to_string(path).expect("read z_image_turbo");
        let ui: Value = serde_json::from_str(&text).expect("parse");
        let api = crate::workflow_convert::ui_to_api(&ui).expect("ui→api");
        let m = detect_mapping(&api);
        assert!(!m.prompt.is_empty(), "prompt should be filled, got empty");
        assert!(!m.seed.is_empty(), "seed should be filled, got empty");
        assert!(!m.steps.is_empty(), "steps should be filled, got empty");
        assert!(!m.width.is_empty(), "width should be filled, got empty");
        assert!(!m.height.is_empty(), "height should be filled, got empty");
        // Spot-check that the chosen field names match the mega-node inputs.
        assert!(
            m.prompt.ends_with(".inputs.text"),
            "prompt path should end with .inputs.text, got {}",
            m.prompt
        );
    }

    #[test]
    fn detect_mapping_stored_z_image_turbo_workflow() {
        // Same test but using the exact API-format workflow stored in DB
        // (post-conversion). Guards against conversion-vs-detection drift.
        let path = "_z_turbo_stored.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("skip: {path} not present");
            return;
        }
        let text = std::fs::read_to_string(path).expect("read stored z_turbo");
        let api: Value = serde_json::from_str(&text).expect("parse");
        let m = detect_mapping(&api);
        assert!(!m.prompt.is_empty(), "prompt should be filled, got empty");
    }

    #[test]
    fn detect_z_image_turbo() {
        // z_image_turbo.json: KSampler=57:3, CLIPTextEncode (positive)=57:27,
        // negative=57:33 (ConditioningZeroOut, no text), EmptySD3LatentImage=57:13.
        let root = std::env::current_dir().unwrap();
        let path = root.join("examples").join("z_image_turbo.json");
        let wf = load(path.to_str().unwrap());

        let m = detect_mapping(&wf);
        assert_eq!(m.prompt, "57:27.inputs.text", "prompt should be direct text link");
        assert_eq!(m.negative_prompt, "", "negative is ConditioningZeroOut, no text");
        assert_eq!(m.width, "57:13.inputs.width");
        assert_eq!(m.height, "57:13.inputs.height");
        assert_eq!(m.batch_size, "57:13.inputs.batch_size");
        assert_eq!(m.seed, "57:3.inputs.seed");
        assert_eq!(m.steps, "57:3.inputs.steps");
        assert_eq!(m.cfg, "57:3.inputs.cfg");
        assert_eq!(m.sampler, "57:3.inputs.sampler_name");
        assert_eq!(m.scheduler, "57:3.inputs.scheduler");
    }

    #[test]
    fn detect_krea2_handles_chained_links() {
        // krea2: KSampler=30:3, positive→30:6 (CLIPTextEncode) whose text is
        // a link ["30:28",0] — i.e. text itself is a chained link.
        // width/height of EmptyLatentImage=30:5 are links to ["49",0]/["49",1],
        // a ResolutionSelector — not numeric at the source either, so width/height
        // remain unmapped.
        let root = std::env::current_dir().unwrap();
        let path = root.join("examples").join("image_krea2_turbo_t2i.json");
        let wf = load(path.to_str().unwrap());

        let m = detect_mapping(&wf);
        // Prompt: text is a link that resolves to node 30:28 which is itself
        // a ComfySwitchNode — its `on_false` is also a link, and we don't
        // follow that. The detector attempts one hop from 30:6.inputs.text
        // to 30:28.inputs.text — but 30:28 has no "text" input (it's a switch),
        // so we expect prompt to remain empty. Admin can wire it manually.
        assert_eq!(m.prompt, "", "prompt should remain empty: chained link through a switch node");
        assert_eq!(m.negative_prompt, "");
        // width/height come from 30:5 EmptyLatentImage, both link arrays.
        // Tracing back to "49" (ResolutionSelector) finds "value" is not "width"/"height",
        // so no numeric path → left empty.
        assert_eq!(m.width, "");
        assert_eq!(m.height, "");
        assert_eq!(m.batch_size, "30:5.inputs.batch_size");
        assert_eq!(m.seed, "30:3.inputs.seed");
        assert_eq!(m.steps, "30:3.inputs.steps");
        assert_eq!(m.cfg, "30:3.inputs.cfg");
        assert_eq!(m.sampler, "30:3.inputs.sampler_name");
        assert_eq!(m.scheduler, "30:3.inputs.scheduler");
    }

    #[test]
    fn detect_no_sampler_returns_empty() {
        let wf = json!({"1": {"inputs": {"x": 1}, "class_type": "Other"}});
        let m = detect_mapping(&wf);
        assert_eq!(m, ParamMapping::default());
    }
}
