import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorSession } from "../application/session.svelte";
import { emptyDocument } from "../../../domain/document";
import type { Script, ScriptPatch } from "../../../shared/rpc/generated/bindings";

const { save } = vi.hoisted(() => ({ save: vi.fn<(patch: ScriptPatch) => Promise<Script>>() }));
vi.mock("../../../shared/rpc/client", () => ({ api: { scripts: { save } } }));

function fixture(): Script {
  return {
    id: "script",
    contentHash: "test-hash",
    title: "Quiet",
    body: "",
    document: JSON.parse(JSON.stringify(emptyDocument())),
    collectionId: "inbox",
    collectionName: "My scripts",
    revision: 1,
    favorite: false,
    lastModelId: null,
    createdAt: "2026-01-01",
    updatedAt: "2026-01-01",
    deletedAt: null,
    session: {
      voiceId: null,
      speed: 1,
      paragraphPauseMs: 2000,
      language: "en-gb",
      ambienceId: null,
      ambienceVolume: 0.06,
      narrationVolume: 1,
      ambienceMode: "loop",
      sleepMinutes: null,
    },
  };
}

afterEach(() => {
  vi.useRealTimers();
  save.mockReset();
});

describe("EditorSession", () => {
  it("serializes saves and does not mark later edits saved", async () => {
    vi.useFakeTimers();
    const original = fixture();
    let finish: (value: Script) => void = () => {};
    save.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    save.mockImplementationOnce(async (patch) => ({ ...original, ...patch, revision: 3 }));
    const session = new EditorSession(() => {});
    session.load(original);
    session.title("First");
    const saving = session.flush();
    session.title("Second");
    expect(session.status).toBe("Unsaved");
    finish({ ...original, title: "First", revision: 2 });
    await saving;
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]?.[0].expectedRevision).toBe(2);
    expect(save.mock.calls[1]?.[0].title).toBe("Second");
    expect(session.script?.title).toBe("Second");
    expect(session.status).toBe("Saved");
    session.dispose();
  });

  it("keeps local work after a rejected save and flush fails", async () => {
    vi.useFakeTimers();
    save.mockRejectedValue(new Error("revision conflict"));
    const session = new EditorSession(() => {});
    session.load(fixture());
    session.title("Keep these words");
    await expect(session.flush()).rejects.toThrow("revision conflict");
    expect(session.script?.title).toBe("Keep these words");
    expect(session.status).toBe("Save failed");
    expect(session.error).toContain("revision conflict");
    session.dispose();
  });
});
