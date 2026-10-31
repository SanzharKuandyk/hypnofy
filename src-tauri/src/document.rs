use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const DOCUMENT_VERSION: u64 = 2;

pub fn from_plain_text(body: &str) -> Value {
    let mut blocks = Vec::new();
    for paragraph in body.split("\n\n") {
        let mut rest = paragraph;
        while let Some(start) = rest.find("[pause:") {
            let after = &rest[start + 7..];
            let parsed = after.find(']').and_then(|end| {
                after[..end]
                    .strip_suffix('s')
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|s| s.is_finite() && (0.5..=120.0).contains(s))
                    .map(|s| (s, end))
            });
            let Some((seconds, end)) = parsed else {
                break;
            };
            if start > 0 {
                blocks.push(paragraph_node(&rest[..start]));
            }
            blocks.push(json!({"type":"pause","attrs":{"id":Uuid::new_v4().to_string(),"durationMs":(seconds*1000.0).round() as u32}}));
            rest = &after[end + 1..];
        }
        if !rest.is_empty() || blocks.is_empty() {
            blocks.push(paragraph_node(rest));
        }
    }
    json!({"version":2,"type":"doc","content":blocks})
}

fn paragraph_node(text: &str) -> Value {
    json!({"type":"paragraph","attrs":{"id":Uuid::new_v4().to_string()},"content":if text.is_empty(){vec![]}else{vec![json!({"type":"text","text":text})]}})
}

pub fn upgrade(mut value: Value) -> Result<Value, String> {
    match value["version"].as_u64() {
        Some(2) => {
            plain_text(&value)?;
            return Ok(value);
        }
        Some(1) => {}
        _ => return Err("Unsupported document version".into()),
    }
    fn walk(v: &mut Value) {
        let kind = v["type"].as_str().unwrap_or("").to_owned();
        if !["doc", "text", "hardBreak"].contains(&kind.as_str()) {
            if !v["attrs"].is_object() {
                v["attrs"] = json!({});
            }
            if v["attrs"]["id"].as_str().is_none() {
                v["attrs"]["id"] = json!(Uuid::new_v4().to_string());
            }
        }
        if kind == "pause" {
            let seconds = v["attrs"]["seconds"].as_f64().unwrap_or(5.0);
            v["attrs"]["durationMs"] = json!((seconds * 1000.0).round() as u32);
            if let Some(attrs) = v["attrs"].as_object_mut() {
                attrs.remove("seconds");
            }
        }
        if let Some(children) = v.get_mut("content").and_then(Value::as_array_mut) {
            for child in children {
                walk(child);
            }
        }
    }
    walk(&mut value);
    value["version"] = json!(2);
    plain_text(&value)?;
    Ok(value)
}

pub fn duplicate(mut value: Value) -> Value {
    fn walk(v: &mut Value, groups: &mut HashMap<String, String>) {
        if v["attrs"]["id"].is_string() {
            v["attrs"]["id"] = json!(Uuid::new_v4().to_string());
        }
        if let Some(group) = v["attrs"]["flowGroupId"].as_str().map(str::to_owned) {
            v["attrs"]["flowGroupId"] = json!(
                groups
                    .entry(group)
                    .or_insert_with(|| Uuid::new_v4().to_string())
            );
        }
        if let Some(children) = v.get_mut("content").and_then(Value::as_array_mut) {
            for child in children {
                walk(child, groups);
            }
        }
    }
    walk(&mut value, &mut HashMap::new());
    value
}

pub fn content_hash(value: &Value) -> String {
    hex(&Sha256::digest(
        serde_json::to_vec(value).unwrap_or_default(),
    ))
}

pub fn text_hash(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

pub fn anchored_text(document: &Value, range: &crate::domain::TextRange) -> Result<String, String> {
    fn blocks<'a>(node: &'a Value, result: &mut Vec<&'a Value>) {
        if ["paragraph", "heading", "pause"].contains(&node["type"].as_str().unwrap_or("")) {
            result.push(node);
        } else if let Some(children) = node["content"].as_array() {
            for node in children {
                blocks(node, result);
            }
        }
    }
    let mut nodes = Vec::new();
    blocks(document, &mut nodes);
    let start = nodes
        .iter()
        .position(|n| n["attrs"]["id"] == range.from.block_id)
        .ok_or("Selection start is missing")?;
    let end = nodes
        .iter()
        .position(|n| n["attrs"]["id"] == range.to.block_id)
        .ok_or("Selection end is missing")?;
    if start > end {
        return Err("Selection anchors are reversed".into());
    }
    let mut text = Vec::new();
    for (index, node) in nodes.iter().enumerate().take(end + 1).skip(start) {
        let plain = if node["type"] == "pause" {
            "\u{fffc}".to_owned()
        } else {
            node["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| match n["type"].as_str() {
                    Some("text") => n["text"].as_str().unwrap_or(""),
                    Some("hardBreak") => "\n",
                    _ => "\u{fffc}",
                })
                .collect::<String>()
        };
        let units = plain.encode_utf16().collect::<Vec<_>>();
        let from = if index == start {
            range.from.offset_utf16 as usize
        } else {
            0
        };
        let to = if index == end {
            range.to.offset_utf16 as usize
        } else {
            units.len()
        };
        let selected = units
            .get(from..to)
            .ok_or("Selection offset is outside the passage")?;
        text.push(
            String::from_utf16(selected).map_err(|_| "Selection splits a Unicode character")?,
        );
    }
    Ok(text.join("\n"))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn plain_text(document: &Value) -> Result<String, String> {
    if document["version"].as_u64() != Some(DOCUMENT_VERSION) || document["type"] != "doc" {
        return Err("Unsupported script document version".into());
    }
    let blocks = document["content"]
        .as_array()
        .ok_or("Script blocks are missing")?;
    let mut ids = HashSet::new();
    let mut count = 0;
    for block in blocks {
        if !["paragraph", "heading", "bulletList", "orderedList", "pause"]
            .contains(&block["type"].as_str().unwrap_or(""))
        {
            return Err("Invalid top-level document node".into());
        }
        validate(block, 0, &mut count, &mut ids)?;
    }
    let text = blocks
        .iter()
        .map(collect_text)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n\n");
    if text.len() > 2_000_000 {
        return Err("Script is larger than 2 MB".into());
    }
    Ok(text)
}

fn validate(
    node: &Value,
    depth: usize,
    count: &mut usize,
    ids: &mut HashSet<String>,
) -> Result<(), String> {
    *count += 1;
    if depth > 32 || *count > 50_000 {
        return Err("Document is too deeply nested or too large".into());
    }
    let kind = node["type"].as_str().ok_or("Node type missing")?;
    if ["bulletList", "orderedList", "listItem"].contains(&kind)
        && node["content"].as_array().is_none_or(Vec::is_empty)
    {
        return Err("Lists and list items require content".into());
    }
    if kind == "text" && node["text"].as_str().is_none_or(str::is_empty) {
        return Err("Text nodes cannot be empty".into());
    }
    if ![
        "paragraph",
        "heading",
        "text",
        "hardBreak",
        "bulletList",
        "orderedList",
        "listItem",
        "pause",
        "inlinePause",
    ]
    .contains(&kind)
    {
        return Err(format!("Unsupported node: {kind}"));
    }
    if kind == "text" {
        if node["text"].as_str().is_none() {
            return Err("Text missing".into());
        }
    } else if !["hardBreak", "inlinePause"].contains(&kind) {
        let id = node["attrs"]["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or("Block ID missing")?;
        if !ids.insert(id.to_owned()) {
            return Err("Duplicate block ID".into());
        }
    }
    if ["pause", "inlinePause"].contains(&kind) {
        let ms = node["attrs"]["durationMs"]
            .as_u64()
            .ok_or("Pause duration missing")?;
        if !(500..=120_000).contains(&ms) {
            return Err("Pause must be between 0.5 and 120 seconds".into());
        }
    }
    if kind == "heading" && !matches!(node["attrs"]["level"].as_u64(), Some(2 | 3)) {
        return Err("Heading must be level 2 or 3".into());
    }
    if node["attrs"]["pauseAfterMs"]
        .as_u64()
        .is_some_and(|ms| ms > 120_000)
    {
        return Err("Passage pause is too long".into());
    }
    check_delivery(&node["attrs"]["delivery"])?;
    if let Some(marks) = node["marks"].as_array() {
        for mark in marks {
            if !["bold", "italic", "strike", "delivery"]
                .contains(&mark["type"].as_str().unwrap_or(""))
            {
                return Err("Unsupported mark".into());
            }
            if mark["type"] == "delivery" {
                check_delivery(&mark["attrs"])?;
            }
        }
    }
    if let Some(content) = node.get("content") {
        for child in content.as_array().ok_or("Content must be an array")? {
            let child_kind = child["type"].as_str().unwrap_or("");
            let allowed = match kind {
                "paragraph" | "heading" => {
                    ["text", "hardBreak", "inlinePause"].contains(&child_kind)
                }
                "bulletList" | "orderedList" => child_kind == "listItem",
                "listItem" => ["paragraph", "heading", "bulletList", "orderedList", "pause"]
                    .contains(&child_kind),
                _ => false,
            };
            if !allowed {
                return Err("Invalid document nesting".into());
            }
            validate(child, depth + 1, count, ids)?;
        }
    }
    Ok(())
}

fn check_delivery(v: &Value) -> Result<(), String> {
    if !v.is_null() && !v.is_object() {
        return Err("Delivery must be an object or inherited".into());
    }
    if let Some(speed) = v.get("speed").filter(|s| !s.is_null()) {
        let speed = speed.as_f64().ok_or("Speed must be a number")?;
        if !speed.is_finite() || !(0.5..=2.0).contains(&speed) {
            return Err("Speed must be between 0.5 and 2.0".into());
        }
    }
    if v.get("voiceId")
        .filter(|s| !s.is_null())
        .is_some_and(|s| s.as_str().is_none_or(|s| s.len() > 256))
    {
        return Err("Invalid voice".into());
    }
    Ok(())
}

pub fn collect_text(node: &Value) -> Result<String, String> {
    match node["type"].as_str().unwrap_or("") {
        "text" => Ok(node["text"].as_str().unwrap_or("").into()),
        "hardBreak" => Ok("\n".into()),
        "pause" | "inlinePause" => Ok(format!(
            "[pause:{}s]",
            node["attrs"]["durationMs"].as_f64().unwrap_or(5000.0) / 1000.0
        )),
        kind => Ok(node["content"]
            .as_array()
            .into_iter()
            .flatten()
            .map(collect_text)
            .collect::<Result<Vec<_>, _>>()?
            .join(
                if ["bulletList", "orderedList", "listItem"].contains(&kind) {
                    "\n"
                } else {
                    ""
                },
            )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_retains_unicode_and_pause() {
        let old = json!({"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Breathe 🌙"}]},{"type":"pause","attrs":{"seconds":4.5}}]});
        let doc = upgrade(old).unwrap();
        assert_eq!(plain_text(&doc).unwrap(), "Breathe 🌙\n\n[pause:4.5s]");
    }

    #[test]
    fn duplication_renews_ids() {
        let doc = from_plain_text("one\n\ntwo");
        let copy = duplicate(doc.clone());
        assert_ne!(
            doc["content"][0]["attrs"]["id"],
            copy["content"][0]["attrs"]["id"]
        );
        let mut invalid = doc.clone();
        invalid["content"][1]["attrs"]["id"] = invalid["content"][0]["attrs"]["id"].clone();
        assert!(plain_text(&invalid).is_err());
    }

    #[test]
    fn unsupported_legacy_is_not_discarded() {
        assert!(
            upgrade(
                json!({"version":1,"type":"doc","content":[{"type":"codeBlock","content":[]}]})
            )
            .is_err()
        );
    }

    #[test]
    fn anchors_use_utf16_and_reject_split_surrogates() {
        use crate::domain::{TextAnchor, TextRange};
        let doc = from_plain_text("a🌙b");
        let id = doc["content"][0]["attrs"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut range = TextRange {
            from: TextAnchor {
                block_id: id.clone(),
                offset_utf16: 1,
            },
            to: TextAnchor {
                block_id: id,
                offset_utf16: 3,
            },
            selected_text_hash: text_hash("🌙"),
        };
        assert_eq!(anchored_text(&doc, &range).unwrap(), "🌙");
        range.to.offset_utf16 = 2;
        assert!(anchored_text(&doc, &range).is_err());
    }
}
