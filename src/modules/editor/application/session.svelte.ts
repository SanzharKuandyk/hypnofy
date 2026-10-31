import { api } from "../../../shared/rpc/client";
import type { Script, SessionSettings } from "../../../domain";
import { decodeDocument, emptyDocument } from "../../../domain/document";
import type { ScriptDocument } from "../../../domain/document";

export class EditorSession {
  script = $state<Script | null>(null);

  document = $state<ScriptDocument>(emptyDocument());

  status = $state("Saved");

  error = $state("");

  editSequence = $state(0);

  savedSequence = $state(0);

  documentEpoch = $state(0);

  private timer: ReturnType<typeof setTimeout> | null = null;

  private saving: Promise<void> | null = null;

  constructor(private readonly onSaved: (script: Script) => void) {}

  load(script: Script): void {
    this.cancelTimer();
    this.document = decodeDocument(script.document);
    this.script = script;
    this.editSequence = 0;
    this.savedSequence = 0;
    this.status = "Saved";
    this.error = "";
    this.documentEpoch++;
  }

  changeDocument(document: ScriptDocument): void {
    this.document = document;
    this.changed();
  }

  title(value: string): void {
    if (this.script) this.script.title = value;
    this.changed();
  }

  session(patch: Partial<SessionSettings>): void {
    if (this.script) Object.assign(this.script.session, patch);
    this.changed();
  }

  metadata(patch: Partial<Pick<Script, "collectionId" | "favorite">>): void {
    if (this.script) Object.assign(this.script, patch);
    this.changed();
  }

  changed(): void {
    this.editSequence++;
    this.status = "Unsaved";
    this.cancelTimer();
    this.timer = setTimeout(() => {
      void this.flush().catch(() => undefined);
    }, 500);
  }

  private cancelTimer(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  async flush(): Promise<void> {
    this.cancelTimer();
    if (this.saving) {
      await this.saving;
      if (this.savedSequence !== this.editSequence) return this.flush();
      return;
    }
    if (!this.script || this.editSequence === this.savedSequence) return;
    const current = $state.snapshot(this.script);
    const document = $state.snapshot(this.document);
    const sequence = this.editSequence;
    this.status = "Saving…";
    this.saving = (async () => {
      try {
        const saved = await api.scripts.save({
          id: current.id,
          title: current.title.trim() || "Untitled script",
          document,
          collectionId: current.collectionId,
          favorite: current.favorite,
          expectedRevision: current.revision,
          session: current.session,
          lastModelId: current.lastModelId,
        });
        if (this.script?.id === saved.id) {
          this.script.revision = saved.revision;
          this.script.contentHash = saved.contentHash;
          this.script.updatedAt = saved.updatedAt;
          this.script.collectionName = saved.collectionName;
          this.script.body = saved.body;
          this.savedSequence = sequence;
          this.status = this.savedSequence === this.editSequence ? "Saved" : "Unsaved";
          this.error = "";
          this.onSaved(saved);
        }
      } catch (value) {
        this.status = "Save failed";
        this.error = value instanceof Error ? value.message : "Saving failed";
        throw value;
      } finally {
        this.saving = null;
      }
    })();
    await this.saving;
    if (this.editSequence !== this.savedSequence) await this.flush();
  }

  dispose(): void {
    this.cancelTimer();
  }
}
