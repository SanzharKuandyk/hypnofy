use serde_json::{Value, json};

pub const DOCUMENT_VERSION: u64 = 1;

/// Convert legacy plain text once, retaining explicit pauses as editable blocks.
pub fn from_plain_text(body: &str) -> Value {
    let mut content = Vec::new();
    for paragraph in body.split("\n\n") {
        let mut remaining = paragraph;
        while let Some(start) = remaining.find("[pause:") {
            let after = &remaining[start + 7..];
            let parsed = after.find(']').and_then(|end| {
                after[..end]
                    .strip_suffix('s')
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| (0.5..=120.0).contains(value))
                    .map(|seconds| (seconds, end))
            });
            let Some((seconds, end)) = parsed else {
                break;
            };
            if start > 0 {
                content.push(json!({"type":"paragraph","content":[{"type":"text","text":&remaining[..start]}]}));
            }
            content.push(json!({"type":"pause","attrs":{"seconds":seconds}}));
            remaining = &after[end + 1..];
        }
        if !remaining.is_empty() || content.is_empty() {
            content.push(json!({"type":"paragraph","content":[{"type":"text","text":remaining}]}));
        }
    }
    json!({"version":DOCUMENT_VERSION,"type":"doc","content":content})
}

/// Accept only the small document vocabulary used by the editor and narration timeline.
pub fn plain_text(document: &Value) -> Result<String, String> {
    if document.get("version").and_then(Value::as_u64) != Some(DOCUMENT_VERSION)
        || document.get("type").and_then(Value::as_str) != Some("doc")
    {
        return Err("Unsupported script document version".into());
    }
    let blocks = document
        .get("content")
        .and_then(Value::as_array)
        .ok_or("Script blocks are missing")?;
    if blocks.len() > 50_000 {
        return Err("Script has too many blocks".into());
    }
    let mut output = Vec::with_capacity(blocks.len());
    for block in blocks {
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or("Script block type is missing")?;
        if kind == "pause" {
            let seconds = block
                .pointer("/attrs/seconds")
                .and_then(Value::as_f64)
                .ok_or("Pause duration is missing")?;
            if !(0.5..=120.0).contains(&seconds) {
                return Err("Pause must be between 0.5 and 120 seconds".into());
            }
            output.push(format!("[pause:{seconds}s]"));
            continue;
        }
        if !["paragraph", "heading", "bulletList", "orderedList"].contains(&kind) {
            return Err(format!("Unsupported script block: {kind}"));
        }
        let text = collect_text(block)?;
        output.push(text);
    }
    let text = output.join("\n\n");
    if text.len() > 2_000_000 {
        return Err("Script is larger than 2 MB".into());
    }
    Ok(text)
}

fn collect_text(node: &Value) -> Result<String, String> {
    if node.get("type").and_then(Value::as_str) == Some("text") {
        return node
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or("Text block has no text".into());
    }
    let mut result = String::new();
    if let Some(children) = node.get("content").and_then(Value::as_array) {
        for child in children {
            if child.get("type").and_then(Value::as_str) == Some("listItem") {
                result.push_str("• ");
            }
            result.push_str(&collect_text(child)?);
            if matches!(
                child.get("type").and_then(Value::as_str),
                Some("listItem" | "paragraph")
            ) && !result.ends_with('\n')
            {
                result.push('\n');
            }
        }
    }
    Ok(result.trim_end_matches('\n').to_owned())
}
