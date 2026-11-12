import type { InlineNode, ScriptDocument, BlockNode, ListItemNode } from "./document";

export type NarrationPart =
  | { readonly kind: "speech"; readonly text: string; readonly from: number; readonly to: number }
  | {
      readonly kind: "silence";
      readonly durationMs: number;
      readonly from: number;
      readonly to: number;
    };

const cuePattern =
  /(?:\[\s*|\(\s*)(?:pause|silence|break)\s*(?::|for)?\s*(\d+(?:\.\d+)?)\s*(?:seconds?|secs?|s)\s*(?:\]|\))|^[ \t]*(?:pause|silence|break)\s*(?::|for)?\s*(\d+(?:\.\d+)?)\s*(?:seconds?|secs?|s)[ \t]*[.!]?[ \t]*\r?$/gim;

export function narrationParts(text: string): NarrationPart[] {
  const parts: NarrationPart[] = [];
  let cursor = 0;
  for (const match of text.matchAll(cuePattern)) {
    const seconds = Number(match[1] ?? match[2]);
    const durationMs = Math.round(seconds * 1000);
    if (!Number.isFinite(seconds) || seconds < 0.5 || seconds > 120)
      throw new Error("Written pauses must be between 0.5 and 120 seconds.");
    if (match.index > cursor)
      parts.push({
        kind: "speech",
        text: text.slice(cursor, match.index),
        from: cursor,
        to: match.index,
      });
    cursor = match.index + match[0].length;
    parts.push({ kind: "silence", durationMs, from: match.index, to: cursor });
  }
  if (cursor < text.length)
    parts.push({ kind: "speech", text: text.slice(cursor), from: cursor, to: text.length });
  const spoken = parts
    .filter((part) => part.kind === "speech")
    .map((part) => part.text)
    .join("");
  if (
    /(?:\[\s*|\(\s*)(?:pause|silence|break)\b|^[ \t]*(?:pause|silence|break)\s*(?::|for)?\s*\d/im.test(
      spoken,
    )
  )
    throw new Error("Use a numeric pause in seconds, such as [pause:5s].");
  return parts;
}

function inlineText(node: InlineNode): string {
  return node.type === "text" ? node.text : node.type === "hardBreak" ? "\n" : "\ufffc";
}

function normalizeInline(nodes: readonly InlineNode[]): InlineNode[] {
  const parts = narrationParts(nodes.map(inlineText).join(""));
  const output: InlineNode[] = [];
  for (const part of parts) {
    if (part.kind === "silence") {
      output.push({ type: "inlinePause", attrs: { durationMs: part.durationMs } });
      continue;
    }
    let offset = 0;
    for (const node of nodes) {
      const end = offset + inlineText(node).length;
      const from = Math.max(offset, part.from);
      const to = Math.min(end, part.to);
      if (from < to)
        output.push(
          node.type === "text"
            ? { ...node, text: node.text.slice(from - offset, to - offset) }
            : structuredClone(node),
        );
      offset = end;
    }
  }
  return output;
}

export function normalizePauseCues(document: ScriptDocument): ScriptDocument {
  function block(node: BlockNode): BlockNode {
    if (node.type === "paragraph" || node.type === "heading")
      return { ...node, content: normalizeInline(node.content ?? []) };
    if (node.type === "bulletList" || node.type === "orderedList")
      return { ...node, content: node.content.map(item) };
    return structuredClone(node);
  }

  function item(node: ListItemNode): ListItemNode {
    return { ...node, content: node.content.map(block) };
  }

  return { ...document, content: document.content.map(block) };
}

export function documentFromDraft(text: string): ScriptDocument {
  return normalizePauseCues({
    version: 2,
    type: "doc",
    content: text
      .trim()
      .split(/\n\s*\n/)
      .filter(Boolean)
      .map((text) => ({
        type: "paragraph",
        attrs: { id: crypto.randomUUID() },
        content: [{ type: "text", text }],
      })),
  });
}
