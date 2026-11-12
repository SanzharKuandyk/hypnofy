//! Converts explicit written pause cues into nonspoken timing nodes.

use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

const PATTERN: &str = r"(?im)(?:\[\s*|\(\s*)(?:pause|silence|break)\s*(?::|for)?\s*(\d+(?:\.\d+)?)\s*(?:seconds?|secs?|s)\s*(?:\]|\))|^[ \t]*(?:pause|silence|break)\s*(?::|for)?\s*(\d+(?:\.\d+)?)\s*(?:seconds?|secs?|s)[ \t]*[.!]?[ \t]*\r?$";

fn matcher() -> Result<&'static Regex, String> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(PATTERN))
        .as_ref()
        .map_err(|error| error.to_string())
}

fn inline_text(node: &Value) -> &str {
    match node["type"].as_str() {
        Some("text") => node["text"].as_str().unwrap_or(""),
        Some("hardBreak") => "\n",
        _ => "\u{fffc}",
    }
}

fn unresolved_matcher() -> Result<&'static Regex, String> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?im)(?:\[\s*|\(\s*)(?:pause|silence|break)\b|^[ \t]*(?:pause|silence|break)\s*(?::|for)?\s*\d")).as_ref().map_err(|error|error.to_string())
}

fn slice(nodes: &[Value], start: usize, end: usize, output: &mut Vec<Value>) {
    let mut offset = 0;
    for node in nodes {
        let text = inline_text(node);
        let next = offset + text.len();
        let from = start.max(offset);
        let to = end.min(next);
        if from < to {
            let mut piece = node.clone();
            if node["type"] == "text" {
                piece["text"] = json!(&text[from - offset..to - offset]);
            }
            output.push(piece);
        }
        offset = next;
    }
}

/// Preserves formatting and block identities while resolving supported pause cues.
pub fn normalize(document: &Value) -> Result<Value, String> {
    fn walk(node: &mut Value, regex: &Regex) -> Result<(), String> {
        if matches!(node["type"].as_str(), Some("paragraph" | "heading")) {
            let Some(nodes) = node["content"].as_array() else {
                return Ok(());
            };
            let text = nodes.iter().map(inline_text).collect::<String>();
            let mut output = Vec::new();
            let mut cursor = 0;
            let mut spoken = String::new();
            for captures in regex.captures_iter(&text) {
                let cue = captures.get(0).ok_or("Invalid pause cue")?;
                let duration = captures
                    .get(1)
                    .or_else(|| captures.get(2))
                    .ok_or("Pause duration missing")?;
                let seconds: f64 = duration
                    .as_str()
                    .parse()
                    .map_err(|_| "Invalid pause duration")?;
                if !seconds.is_finite() || !(0.5..=120.0).contains(&seconds) {
                    return Err("Written pauses must be between 0.5 and 120 seconds. Edit the pause cue before narrating.".into());
                }
                slice(nodes, cursor, cue.start(), &mut output);
                spoken.push_str(&text[cursor..cue.start()]);
                output.push(json!({"type":"inlinePause","attrs":{"durationMs":(seconds*1000.0).round() as u32}}));
                cursor = cue.end();
            }
            spoken.push_str(&text[cursor..]);
            if unresolved_matcher()?.is_match(&spoken) {
                return Err("A pause direction needs numeric seconds, for example [pause:5s]. Edit it before narrating.".into());
            }
            if cursor > 0 {
                slice(nodes, cursor, text.len(), &mut output);
                node["content"] = json!(output);
            }
        } else if let Some(children) = node["content"].as_array_mut() {
            for child in children {
                walk(child, regex)?;
            }
        }
        Ok(())
    }
    let mut normalized = document.clone();
    walk(&mut normalized, matcher()?)?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_fixtures_preserve_speech_and_resolve_only_explicit_directions() {
        let fixtures: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/documents/pause-cues.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let doc = json!({"version":2,"type":"doc","content":[{"type":"paragraph","attrs":{"id":"stable"},"content":[{"type":"text","text":fixture["text"]}]}]});
            let result = normalize(&doc);
            if fixture["invalid"] == true {
                assert!(result.is_err(), "{fixture}");
                continue;
            }
            let normalized = result.unwrap();
            let nodes = normalized["content"][0]["content"].as_array().unwrap();
            let speech: String = nodes.iter().filter_map(|n| n["text"].as_str()).collect();
            let pauses: Vec<u64> = nodes
                .iter()
                .filter_map(|n| n["attrs"]["durationMs"].as_u64())
                .collect();
            assert_eq!(speech, fixture["speech"].as_str().unwrap(), "{fixture}");
            assert_eq!(json!(pauses), fixture["pauses"], "{fixture}");
            assert_eq!(normalized["content"][0]["attrs"]["id"], "stable");
        }
    }
}
