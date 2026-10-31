use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::{Value, json};

fn block(kind: &str) -> Value {
    json!({"type":kind,"attrs":{"id":uuid::Uuid::new_v4().to_string()},"content":[]})
}

fn push(stack: &mut [Value], node: Value) {
    if let Some(parent) = stack.last_mut()
        && let Some(content) = parent["content"].as_array_mut()
    {
        content.push(node);
    }
}

fn finish(stack: &mut Vec<Value>) {
    if stack.len() > 1
        && let Some(node) = stack.pop()
    {
        push(stack, node);
    }
}

fn inline(text: &str, marks: &[Value], stack: &mut [Value]) {
    let mut remaining = text;
    while let Some(start) = remaining.find("[pause:") {
        let rest = &remaining[start + 7..];
        let pause = rest.find(']').and_then(|end| {
            rest[..end]
                .strip_suffix('s')
                .and_then(|n| n.parse::<f64>().ok())
                .filter(|n| n.is_finite() && (0.5..=120.0).contains(n))
                .map(|n| (n, end))
        });
        let Some((seconds, end)) = pause else { break };
        if start > 0 {
            push(
                stack,
                json!({"type":"text","text":&remaining[..start],"marks":marks}),
            );
        }
        push(
            stack,
            json!({"type":"inlinePause","attrs":{"durationMs":(seconds*1000.0).round() as u32}}),
        );
        remaining = &rest[end + 1..];
    }
    if !remaining.is_empty() {
        push(stack, json!({"type":"text","text":remaining,"marks":marks}));
    }
}

/// Parse Markdown structure; formatting syntax never becomes narration text.
pub fn import(text: &str) -> Value {
    let mut stack = vec![json!({"version":2,"type":"doc","content":[]})];
    let mut marks = Vec::new();
    for event in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(Tag::Paragraph) => stack.push(block("paragraph")),
            Event::Start(Tag::Heading { level, .. }) => {
                let mut node = block("heading");
                node["attrs"]["level"] = json!(if level as u8 >= 3 { 3 } else { 2 });
                stack.push(node);
            }
            Event::Start(Tag::List(start)) => {
                let mut node = block(if start.is_some() {
                    "orderedList"
                } else {
                    "bulletList"
                });
                node["attrs"]["start"] = json!(start.unwrap_or(1));
                stack.push(node);
            }
            Event::Start(Tag::Item) => stack.push(block("listItem")),
            Event::Start(Tag::Strong) => marks.push(json!({"type":"bold"})),
            Event::Start(Tag::Emphasis) => marks.push(json!({"type":"italic"})),
            Event::Start(Tag::Strikethrough) => marks.push(json!({"type":"strike"})),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                marks.pop();
            }
            Event::Text(text) | Event::Code(text) => {
                // Tight Markdown lists omit paragraph events.
                if stack.last().is_some_and(|n| n["type"] == "listItem") {
                    stack.push(block("paragraph"));
                }
                inline(&text, &marks, &mut stack);
            }
            Event::SoftBreak => inline(" ", &marks, &mut stack),
            Event::HardBreak => push(&mut stack, json!({"type":"hardBreak"})),
            Event::End(TagEnd::Item) => {
                if stack.last().is_some_and(|n| n["type"] == "paragraph") {
                    finish(&mut stack);
                }
                finish(&mut stack);
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::List(_)) => {
                finish(&mut stack)
            }
            _ => {}
        }
    }
    while stack.len() > 1 {
        finish(&mut stack);
    }
    let document = stack
        .pop()
        .unwrap_or_else(|| crate::document::from_plain_text(""));
    if document["content"].as_array().is_none_or(Vec::is_empty) {
        crate::document::from_plain_text(text)
    } else {
        document
    }
}

pub fn export(document: &Value) -> String {
    fn walk(node: &Value, indent: usize) -> String {
        let children = || {
            node["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|child| walk(child, indent))
                .collect::<String>()
        };
        match node["type"].as_str().unwrap_or("") {
            "text" => {
                let mut text = node["text"]
                    .as_str()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('*', "\\*")
                    .replace('_', "\\_");
                for mark in node["marks"].as_array().into_iter().flatten() {
                    text = match mark["type"].as_str() {
                        Some("bold") => format!("**{text}**"),
                        Some("italic") => format!("*{text}*"),
                        Some("strike") => format!("~~{text}~~"),
                        _ => text,
                    };
                }
                text
            }
            "heading" => format!(
                "{} {}",
                if node["attrs"]["level"] == 3 {
                    "###"
                } else {
                    "##"
                },
                children()
            ),
            "pause" | "inlinePause" => crate::document::collect_text(node).unwrap_or_default(),
            "hardBreak" => "  \n".into(),
            "bulletList" | "orderedList" => node["content"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, item)| {
                    let marker = if node["type"] == "orderedList" {
                        format!(
                            "{}.",
                            node["attrs"]["start"].as_u64().unwrap_or(1) + i as u64
                        )
                    } else {
                        "-".into()
                    };
                    let text = walk(item, indent + 2);
                    format!("{}{} {}", " ".repeat(indent), marker, text.trim_start())
                })
                .collect::<Vec<_>>()
                .join("\n"),
            kind => node["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|child| walk(child, indent))
                .collect::<Vec<_>>()
                .join(if kind == "doc" {
                    "\n\n"
                } else if kind == "listItem" {
                    "\n"
                } else {
                    ""
                }),
        }
    }
    walk(document, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_markdown_lists_marks_unicode_and_pauses() {
        let doc = import(
            "## Arriving\n\n**Breathe** gently 🌙. [pause:2s]\n\n- First\n- Second\n\n1. One\n2. Two",
        );
        let plain = crate::document::plain_text(&doc).unwrap();
        assert!(plain.contains("Breathe gently 🌙."));
        assert!(plain.contains("[pause:2s]"));
        assert!(!plain.contains("**"));
        assert!(
            doc["content"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["type"] == "bulletList")
        );
        let again = import(&export(&doc));
        assert_eq!(crate::document::plain_text(&again).unwrap(), plain);
    }
}
