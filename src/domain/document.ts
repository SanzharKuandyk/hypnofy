export interface DeliveryOverride {
  voiceId?: string | null;

  speed?: number | null;
}

export type TextMark =
  { type: "bold" | "italic" | "strike" } | { type: "delivery"; attrs: DeliveryOverride };

export type InlineNode =
  | { type: "text"; text: string; marks?: TextMark[] }
  | { type: "hardBreak" }
  | { type: "inlinePause"; attrs: { durationMs: number } };

export interface BlockAttributes {
  id: string;

  delivery?: DeliveryOverride | null;

  flowGroupId?: string | null;

  pauseAfterMs?: number | null;
}

export type BlockNode =
  | { type: "paragraph"; attrs: BlockAttributes; content?: InlineNode[] }
  | { type: "heading"; attrs: BlockAttributes & { level: 2 | 3 }; content?: InlineNode[] }
  | {
      type: "bulletList" | "orderedList";
      attrs: BlockAttributes & { start?: number };
      content: ListItemNode[];
    }
  | { type: "pause"; attrs: BlockAttributes & { durationMs: number } };

export interface ListItemNode {
  type: "listItem";

  attrs: BlockAttributes;

  content: BlockNode[];
}

export type DocumentNode = BlockNode | ListItemNode | InlineNode;

export interface ScriptDocument {
  version: 2;

  type: "doc";

  content: BlockNode[];
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function delivery(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  return (
    record(value) &&
    (value.voiceId === undefined ||
      value.voiceId === null ||
      (typeof value.voiceId === "string" && value.voiceId.length <= 256)) &&
    (value.speed === undefined ||
      value.speed === null ||
      (typeof value.speed === "number" &&
        Number.isFinite(value.speed) &&
        value.speed >= 0.5 &&
        value.speed <= 2))
  );
}

export function isDocument(value: unknown): value is ScriptDocument {
  if (
    !record(value) ||
    value.version !== 2 ||
    value.type !== "doc" ||
    !Array.isArray(value.content)
  )
    return false;
  const ids = new Set<string>();
  let count = 0;
  const walk = (node: unknown, depth: number): boolean => {
    if (
      !record(node) ||
      depth > 32 ||
      ++count > 50000 ||
      typeof node.type !== "string" ||
      ![
        "paragraph",
        "heading",
        "bulletList",
        "orderedList",
        "listItem",
        "text",
        "hardBreak",
        "pause",
        "inlinePause",
      ].includes(node.type)
    )
      return false;
    if (node.type === "text" && (typeof node.text !== "string" || !node.text)) return false;
    if (!["text", "hardBreak", "inlinePause"].includes(node.type)) {
      if (
        !record(node.attrs) ||
        typeof node.attrs.id !== "string" ||
        !node.attrs.id ||
        node.attrs.id.length > 128 ||
        ids.has(node.attrs.id)
      )
        return false;
      ids.add(node.attrs.id);
    }
    if (
      node.type === "heading" &&
      (!record(node.attrs) || (node.attrs.level !== 2 && node.attrs.level !== 3))
    )
      return false;
    if (
      ["bulletList", "orderedList", "listItem"].includes(node.type) &&
      (!Array.isArray(node.content) || !node.content.length)
    )
      return false;
    if (record(node.attrs) && !delivery(node.attrs.delivery)) return false;
    if (
      node.marks !== undefined &&
      (!Array.isArray(node.marks) ||
        !node.marks.every(
          (mark) =>
            record(mark) &&
            ["bold", "italic", "strike", "delivery"].includes(String(mark.type)) &&
            (mark.type !== "delivery" || delivery(mark.attrs)),
        ))
    )
      return false;
    if (
      ["pause", "inlinePause"].includes(node.type) &&
      (!record(node.attrs) ||
        typeof node.attrs.durationMs !== "number" ||
        !Number.isSafeInteger(node.attrs.durationMs) ||
        node.attrs.durationMs < 500 ||
        node.attrs.durationMs > 120000)
    )
      return false;
    const allowed =
      node.type === "paragraph" || node.type === "heading"
        ? ["text", "hardBreak", "inlinePause"]
        : node.type === "bulletList" || node.type === "orderedList"
          ? ["listItem"]
          : node.type === "listItem"
            ? ["paragraph", "heading", "bulletList", "orderedList", "pause"]
            : [];
    return (
      node.content === undefined ||
      (Array.isArray(node.content) &&
        node.content.every(
          (child) =>
            record(child) && allowed.includes(String(child.type)) && walk(child, depth + 1),
        ))
    );
  };
  return value.content.every(
    (node) =>
      record(node) &&
      ["paragraph", "heading", "bulletList", "orderedList", "pause"].includes(String(node.type)) &&
      walk(node, 0),
  );
}

export function decodeDocument(value: unknown): ScriptDocument {
  if (!isDocument(value))
    throw new Error("This document needs recovery. Its original content is preserved.");
  return structuredClone(value);
}

export function emptyDocument(): ScriptDocument {
  return {
    version: 2,
    type: "doc",
    content: [{ type: "paragraph", attrs: { id: crypto.randomUUID() }, content: [] }],
  };
}

export function textOf(node: DocumentNode | ScriptDocument): string {
  if (node.type === "text") return node.text ?? "";
  if (node.type === "pause" || node.type === "inlinePause")
    return `[pause:${(node.attrs?.durationMs ?? 5000) / 1000}s]`;
  if (node.type === "hardBreak") return "\n";
  return ("content" in node ? (node.content ?? []) : [])
    .map(textOf)
    .join(["doc", "listItem", "bulletList", "orderedList"].includes(node.type) ? "\n\n" : "");
}

export function countWords(doc: ScriptDocument): number {
  return textOf(doc)
    .replace(/\[pause:[^\]]+\]/g, "")
    .trim()
    .split(/\s+/)
    .filter(Boolean).length;
}
