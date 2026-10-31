import { describe, expect, it } from "vitest";
import { decodeDocument, emptyDocument, isDocument, countWords, textOf } from "./document";

describe("versioned documents", () => {
  it("rejects duplicate IDs, malformed pauses and unsupported versions", () => {
    const doc = emptyDocument();
    expect(isDocument(doc)).toBe(true);
    expect(isDocument({ ...doc, version: 1 })).toBe(false);
    expect(isDocument({ ...doc, content: [...doc.content, ...doc.content] })).toBe(false);
    expect(
      isDocument({ ...doc, content: [{ type: "pause", attrs: { id: "pause", durationMs: 1 } }] }),
    ).toBe(false);
    expect(() => decodeDocument({ version: 1 })).toThrow("recovery");
  });

  it("retains Unicode and pause markers through the codec", () => {
    const doc = emptyDocument();
    const paragraph = doc.content[0];
    if (!paragraph || paragraph.type !== "paragraph") throw new Error("Missing paragraph");
    paragraph.content = [
      { type: "text", text: "Breathe gently 🌙" },
      { type: "inlinePause", attrs: { durationMs: 2500 } },
    ];
    const decoded = decodeDocument(JSON.parse(JSON.stringify(doc)));
    expect(textOf(decoded)).toContain("[pause:2.5s]");
    expect(countWords(decoded)).toBe(3);
  });
});
