import { api, subscribeJobs } from "../../shared/rpc/client";
import { applyTheme } from "../../shared/theme/theme";
import { EditorSession } from "../../modules/editor";
import { chooseScript, chooseExport, chooseRecoveryExport } from "../../shared/platform/dialogs";
import type { Route } from "../routes";
import type {
  Script,
  Collection,
  ScriptSummary,
  ModelRecord,
  Preferences,
  JobSnapshot,
  Suggestion,
  RenderRecord,
  PlaybackState,
  Ambience,
} from "../../domain";

export class Workspace {
  route = $state<Route>({ kind: "library", view: "all", collectionId: null });
  collections = $state<Collection[]>([]);
  scripts = $state<ScriptSummary[]>([]);
  models = $state<ModelRecord[]>([]);
  preferences = $state<Preferences | null>(null);
  jobs = $state<JobSnapshot[]>([]);
  suggestions = $state<Suggestion[]>([]);
  renders = $state<RenderRecord[]>([]);
  ambience = $state<Ambience[]>([]);
  playback = $state<PlaybackState | null>(null);
  query = $state("");
  notice = $state(false);
  help = $state(false);
  loading = $state(true);
  busy = $state(false);
  error = $state("");
  toast = $state("");
  undoTrash = $state<string[]>([]);
  recovery = $state<Script | null>(null);
  recoveryOpen = $state(false);
  editor = new EditorSession(() => {
    void this.refreshLibrary().catch((value) => this.report(value));
  });
  private toastTimer: ReturnType<typeof setTimeout> | null = null;
  private refreshing = false;
  private unsubscribe: (() => void) | null = null;
  private librarySequence = 0;
  private navigationSequence = 0;

  report(value: unknown): void {
    this.error = value instanceof Error ? value.message : "The operation could not be completed";
  }

  async action(work: () => Promise<void>): Promise<void> {
    this.error = "";
    this.busy = true;
    try {
      await work();
    } catch (value) {
      this.report(value);
    } finally {
      this.busy = false;
    }
  }

  notify(message: string): void {
    this.toast = message;
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = ""), 6000);
  }

  async initialize(): Promise<void> {
    try {
      this.unsubscribe = await subscribeJobs((snapshot) => {
        const index = this.jobs.findIndex((job) => job.id === snapshot.id);
        if (index < 0) this.jobs = [...this.jobs, snapshot];
        else if ((this.jobs[index]?.sequence ?? -1) < snapshot.sequence)
          this.jobs[index] = snapshot;
        if (snapshot.state === "failed" && snapshot.error)
          this.report(new Error(snapshot.error.message));
        if (snapshot.state === "completed" && snapshot.kind === "audio-export")
          this.notify("WAV exported");
        if (snapshot.state === "completed" && snapshot.kind === "script-export")
          this.notify("Scripts exported");
        if (["completed", "failed", "cancelled"].includes(snapshot.state))
          void this.refreshResults().catch((value) => this.report(value));
      });
      const [entry, preferences] = await Promise.all([api.entry.status(), api.settings.get()]);
      this.notice = !entry.accepted;
      this.preferences = preferences;
      applyTheme(
        preferences.theme === "light" || preferences.theme === "system"
          ? preferences.theme
          : "dark",
      );
      await this.refresh();
    } catch (value) {
      this.report(value);
    } finally {
      this.loading = false;
    }
  }

  async accept(): Promise<void> {
    await api.entry.accept("2026-11-01", true);
    this.notice = false;
  }

  async refreshLibrary(): Promise<void> {
    const sequence = ++this.librarySequence;
    const route = this.route;
    const [collections, scripts] = await Promise.all([
      api.library.collections(),
      api.library.scripts({
        collectionId: route.kind === "library" ? route.collectionId : null,
        query: this.query,
        favorite: route.kind === "library" && route.view === "favorites",
        trash: route.kind === "library" && route.view === "trash",
      }),
    ]);
    if (sequence !== this.librarySequence) return;
    this.collections = collections;
    this.scripts = scripts;
  }

  async refresh(): Promise<void> {
    await this.refreshLibrary();
    [this.models, this.jobs, this.ambience] = await Promise.all([
      api.models.list(),
      api.jobs.list(),
      api.playback.ambience(),
    ]);
  }

  async poll(): Promise<void> {
    if (this.refreshing || this.notice) return;
    this.refreshing = true;
    try {
      const jobs = await api.jobs.list();
      const changed = jobs.some(
        (job) => this.jobs.find((old) => old.id === job.id)?.sequence !== job.sequence,
      );
      this.jobs = jobs;
      this.playback = await api.playback.state();
      if (changed) {
        this.models = await api.models.list();
        if (this.route.kind === "script") {
          [this.suggestions, this.renders] = await Promise.all([
            api.generation.suggestions(this.route.id),
            api.narration.renders(this.route.id),
          ]);
        }
      }
    } catch (value) {
      this.report(value);
    } finally {
      this.refreshing = false;
    }
  }

  private async refreshResults(): Promise<void> {
    this.models = await api.models.list();
    if (this.route.kind === "script")
      [this.suggestions, this.renders] = await Promise.all([
        api.generation.suggestions(this.route.id),
        api.narration.renders(this.route.id),
      ]);
  }

  async navigate(route: Route): Promise<void> {
    const sequence = ++this.navigationSequence;
    await this.editor.flush();
    if (sequence !== this.navigationSequence) return;
    if (route.kind === "script") {
      const [script, suggestions, renders] = await Promise.all([
        api.scripts.get(route.id),
        api.generation.suggestions(route.id),
        api.narration.renders(route.id),
      ]);
      if (sequence !== this.navigationSequence) return;
      try {
        this.editor.load(script);
      } catch (value) {
        if (await api.scripts.legacyDocument(route.id)) {
          if (sequence !== this.navigationSequence) return;
          this.recovery = script;
          this.recoveryOpen = true;
          return;
        }
        throw value;
      }
      this.suggestions = suggestions;
      this.renders = renders;
    }
    this.route = route;
    await this.refreshLibrary();
  }

  async createScript(): Promise<void> {
    const sequence = ++this.navigationSequence;
    await this.editor.flush();
    if (sequence !== this.navigationSequence) return;
    const script = await api.scripts.create(
      this.route.kind === "library" ? this.route.collectionId : null,
    );
    if (sequence !== this.navigationSequence) return;
    if (this.preferences) {
      script.session.voiceId = this.preferences.voiceId;
      script.session.speed = this.preferences.speed;
      script.session.paragraphPauseMs = this.preferences.paragraphPauseMs;
      script.session.language = this.preferences.language;
      script.session.narrationVolume = this.preferences.narrationVolume;
      script.session.ambienceVolume = this.preferences.ambienceVolume;
    }
    this.editor.load(script);
    this.editor.changed();
    await this.editor.flush();
    if (sequence !== this.navigationSequence) return;
    this.route = { kind: "script", id: script.id };
    this.suggestions = [];
    this.renders = [];
    await this.refreshLibrary();
  }

  async bulk(ids: string[], operation: string, collectionId: string | null = null): Promise<void> {
    await this.editor.flush();
    const result = await api.scripts.bulk({ ids, operation, collectionId });
    await this.refreshAudio();
    await this.refreshLibrary();
    this.notify(
      result.pendingAudioDeletions
        ? "Script deleted. Locked audio removal is pending and will retry at startup or cleanup."
        : operation === "trash"
          ? "Moved to Trash. Restore it from Trash at any time."
          : "Library updated",
    );
    this.undoTrash = operation === "trash" ? [...ids] : [];
  }

  async refreshAudio(): Promise<void> {
    this.playback = await api.playback.state();
    if (this.editor.script) this.renders = await api.narration.renders(this.editor.script.id);
  }

  async importScript(): Promise<void> {
    await this.editor.flush();
    const path = await chooseScript();
    if (!path) return;
    const script = await api.scripts.importFile(
      path,
      this.route.kind === "library" ? this.route.collectionId : null,
    );
    await this.navigate({ kind: "script", id: script.id });
  }

  async exportScript(id: string, title: string): Promise<void> {
    await this.editor.flush();
    const path = await chooseExport(title);
    if (path) {
      await api.scripts.exportFile(id, path);
      this.notify("Script exported");
    }
  }

  async renameScript(id: string, title: string): Promise<void> {
    await this.editor.flush();
    const script = await api.scripts.get(id);
    await api.scripts.save({
      id,
      title: title.trim(),
      document: script.document,
      collectionId: script.collectionId,
      favorite: script.favorite,
      expectedRevision: script.revision,
      session: script.session,
      lastModelId: script.lastModelId,
    });
    await this.refreshLibrary();
  }

  async exportRecovery(): Promise<void> {
    if (!this.recovery) return;
    const path = await chooseRecoveryExport(this.recovery.title);
    if (path) {
      await api.scripts.exportFile(this.recovery.id, path);
      this.notify("Original script exported for recovery");
    }
  }

  async savePreferences(patch: Partial<Preferences>): Promise<void> {
    if (!this.preferences) return;
    this.preferences = await api.settings.save({ ...$state.snapshot(this.preferences), ...patch });
    const theme = this.preferences.theme;
    applyTheme(theme === "light" || theme === "system" ? theme : "dark");
  }

  dispose(): void {
    this.unsubscribe?.();
    this.editor.dispose();
    if (this.toastTimer) clearTimeout(this.toastTimer);
  }
}
