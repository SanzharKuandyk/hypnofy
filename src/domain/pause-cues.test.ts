import { describe, expect, it } from "vitest";
import fixtures from "../../tests/fixtures/documents/pause-cues.json";
import { narrationParts, normalizePauseCues } from "./pause-cues";
import type { ScriptDocument } from "./document";

describe("narration cue conversion", () => {
  for (const fixture of fixtures)
    it(fixture.text, () => {
      if (fixture.invalid) {
        expect(() => narrationParts(fixture.text)).toThrow();
        return;
      }
      const parts = narrationParts(fixture.text);
      expect(parts.filter((p) => p.kind === "silence").map((p) => p.durationMs)).toEqual(
        fixture.pauses,
      );
      expect(
        parts
          .filter((p) => p.kind === "speech")
          .map((p) => p.text)
          .join(""),
      ).toBe(fixture.speech);
    });

  it("recognizes cues across formatting boundaries and preserves block IDs and delivery", () => {
    const document: ScriptDocument = {
      version: 2,
      type: "doc",
      content: [
        {
          type: "paragraph",
          attrs: { id: "stable" },
          content: [
            { type: "text", text: "🌙 [pause " },
            { type: "text", text: "5", marks: [{ type: "bold" }] },
            {
              type: "text",
              text: " seconds] Rest.",
              marks: [{ type: "delivery", attrs: { speed: 0.8 } }],
            },
          ],
        },
      ],
    };
    const normalized = normalizePauseCues(document);
    const block = normalized.content[0];
    expect(block?.attrs.id).toBe("stable");
    if (!block || block.type !== "paragraph") throw new Error("Missing paragraph");
    expect(block.content).toEqual([
      { type: "text", text: "🌙 " },
      { type: "inlinePause", attrs: { durationMs: 5000 } },
      { type: "text", text: " Rest.", marks: [{ type: "delivery", attrs: { speed: 0.8 } }] },
    ]);
    expect(document.content[0]).not.toEqual(block);
  });
});
