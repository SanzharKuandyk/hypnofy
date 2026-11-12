import { test, expect, chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtemp, rm, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { startModelHubFixture } from "../fixtures/providers/model-hub";
import type { ModelHubFixture } from "../fixtures/providers/model-hub";
import type { ChildProcess } from "node:child_process";
import type { Browser, Page } from "@playwright/test";
import type {
  ModelRecord,
  JobSnapshot,
  Script,
  ScriptPatch,
  RenderRecord,
  Preferences,
  ScriptSummary,
  PlaybackState,
  Ambience,
  StoredRecording,
} from "../../src/shared/rpc/generated/bindings";

declare global {
  interface Window {
    __TAURI_INTERNALS__: { invoke<T>(command: string, args: Record<string, unknown>): Promise<T> };
  }
}

let process: ChildProcess;
let browser: Browser;
let page: Page;
let root: string;
const errors: string[] = [];
let nativeDiagnostics = "";
let hub: ModelHubFixture;
test.describe.configure({ mode: "serial" });

async function native<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  return page.evaluate(
    ({ name, args }) => window.__TAURI_INTERNALS__.invoke<T>(`TauRPC__${name}`, args),
    { name, args },
  );
}

async function startApp(): Promise<void> {
  hub ??= await startModelHubFixture();
  nativeDiagnostics = "";
  root = await mkdtemp(join(tmpdir(), "hypnofy-e2e-"));
  const port = 19100 + Math.floor(Math.random() * 1000);
  process = spawn(resolve(".b/debug/Hypnofy-dev.exe"), [], {
    windowsHide: true,
    env: {
      ...globalThis.process.env,
      HYPNOFY_TEST_DATA_DIR: join(root, "workspace"),
      HYPNOFY_TEST_HUB_URL: hub.url,
      WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  process.stdout?.on("data", (data: Buffer) => {
    nativeDiagnostics = (nativeDiagnostics + data.toString()).slice(-12000);
  });
  process.stderr?.on("data", (data: Buffer) => {
    nativeDiagnostics = (nativeDiagnostics + data.toString()).slice(-12000);
  });
  process.on("exit", (code, signal) => console.log(`Native process exited: ${code ?? signal}`));
  await expect
    .poll(
      async () =>
        fetch(`http://127.0.0.1:${port}/json/version`)
          .then((r) => r.ok)
          .catch(() => false),
      { timeout: 45000 },
    )
    .toBe(true);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const context = browser.contexts()[0];
  if (!context) throw new Error("No native WebView context");
  page = context.pages()[0] ?? (await context.waitForEvent("page"));
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("crash", () => console.log("Native WebView renderer crashed"));
  await page.waitForFunction(() => typeof window.__TAURI_INTERNALS__?.invoke === "function");
}

test.beforeAll(startApp);

async function stopApp(): Promise<void> {
  if (nativeDiagnostics) console.log(nativeDiagnostics);
  await browser?.close();
  process?.kill();
  if (process && process.exitCode === null && process.signalCode === null)
    await new Promise<void>((resolve) => process.once("exit", () => resolve()));
  if (root) await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
}

test.afterAll(async () => {
  await stopApp();
  await hub?.close();
});

test("entry is required and survives a frontend reload", async () => {
  await expect(page.getByRole("heading", { name: "Before you begin" })).toBeVisible();
  const blocked = await page.evaluate(() =>
    window.__TAURI_INTERNALS__
      .invoke("TauRPC__playback.control", { operation: "play", value: null })
      .then(
        () => null,
        (error) => error,
      ),
  );
  expect(blocked).toMatchObject({ code: "entry-required" });
  const accept = page.getByRole("button", { name: "Accept & continue" });
  await expect(accept).toBeDisabled();
  await page.setViewportSize({ width: 390, height: 620 });
  await expect(accept).toBeInViewport();
  await expect(
    page.getByRole("checkbox", { name: "I have read the welcome notice" }),
  ).toBeInViewport();
  await page.keyboard.press("Escape");
  await expect(accept).toBeVisible();
  await page.getByRole("checkbox", { name: "I have read the welcome notice" }).click();
  await expect(accept).toBeEnabled();
  await accept.click();
  await expect(page.getByRole("heading", { name: "All scripts", exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("heading", { name: "All scripts", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Before you begin" })).toHaveCount(0);
  await page.setViewportSize({ width: 1440, height: 960 });
});

test("real local models generate, narrate and export WAV through the native app", async () => {
  test.skip(
    globalThis.process.env.HYPNOFY_REAL_MODELS !== "1",
    "Explicit real-model integration suite",
  );
  test.setTimeout(300000);
  const speech = await native<ModelRecord>("models.link", {
    path: resolve(".b/smoke-models/speech"),
    architecture: "kokoro",
    existing_id: null,
  });
  const text = await native<ModelRecord>("models.link", {
    path: resolve(".b/smoke-models/text/model.gguf"),
    architecture: null,
    existing_id: null,
  });
  for (const model of [speech, text]) {
    const jobId = crypto.randomUUID();
    await native("models.verify", { id: model.id, job_id: jobId });
    await expect
      .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: jobId })).state, {
        timeout: 180000,
        intervals: [250, 500],
      })
      .toBe("completed");
  }
  const ready = await native<ModelRecord[]>("models.list");
  expect(ready.find((m) => m.id === text.id)?.contextTokens).toBeGreaterThanOrEqual(2048);
  const voice = ready.find((m) => m.id === speech.id)?.voices[0];
  if (!voice) throw new Error("Verified voice missing");
  const script = await native<Script>("scripts.create", { collection_id: null });
  const document = {
    version: 2,
    type: "doc",
    content: [
      {
        type: "paragraph",
        attrs: { id: crypto.randomUUID() },
        content: [{ type: "text", text: "Take a gentle breath." }],
      },
      { type: "pause", attrs: { id: crypto.randomUUID(), durationMs: 1500 } },
      {
        type: "paragraph",
        attrs: { id: crypto.randomUUID(), delivery: { speed: 0.85, voiceId: null } },
        content: [{ type: "text", text: "Rest for a moment." }],
      },
    ],
  };
  const patch: ScriptPatch = {
    id: script.id,
    title: "Native audio fixture",
    document,
    expectedRevision: script.revision,
    collectionId: script.collectionId,
    favorite: false,
    lastModelId: text.id,
    session: { ...script.session, voiceId: voice.id, speed: 1 },
  };
  const saved = await native<Script>("scripts.save", { patch });
  const generation = crypto.randomUUID();
  await native("generation.start", {
    request: {
      jobId: generation,
      scriptId: saved.id,
      baseRevision: saved.revision,
      baseContentHash: saved.contentHash,
      anchor: null,
      range: null,
      modelId: text.id,
      operation: "continue",
      instruction: "Write one short calming sentence, then stop. /no_think",
      selectedText: "",
    },
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: generation })).state, {
      timeout: 180000,
    })
    .toBe("completed");
  expect(
    (await native<{ text: string }[]>("generation.suggestions", { script_id: saved.id }))[0]?.text
      .length,
  ).toBeGreaterThan(5);
  const narration = crypto.randomUUID();
  await native("narration.render", {
    script_id: saved.id,
    job_id: narration,
    preview: false,
    block_ids: null,
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: narration })).state, {
      timeout: 180000,
    })
    .toBe("completed");
  const renders = await native<RenderRecord[]>("narration.renders", { script_id: saved.id });
  const render = renders[0];
  if (!render) throw new Error("Render missing");
  expect(render.durationMs).toBeGreaterThan(2000);
  const path = join(root, "narration.wav");
  const exportId = crypto.randomUUID();
  await native("narration.export", {
    render_id: render.id,
    path,
    ambience_id: null,
    job_id: exportId,
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: exportId })).state)
    .toBe("completed");
  const audio = await readFile(path);
  expect(audio.subarray(0, 4).toString()).toBe("RIFF");
  expect(audio.readUInt32LE(24)).toBe(48000);
  expect(audio.readUInt16LE(22)).toBe(1);
  expect(audio.readUInt16LE(34)).toBe(16);
  await native("playback.load", { render_id: render.id });
  await native("playback.control", { operation: "play", value: null });
  await expect
    .poll(async () => (await native<{ positionMs: number }>("playback.state")).positionMs)
    .toBeGreaterThan(100);
  await native("playback.control", { operation: "pause", value: null });
  await native("playback.control", { operation: "seek", value: 1000 });
  await native("playback.control", { operation: "stop", value: null });
  const cacheBefore = await readdir(join(root, "workspace/audio/chunks"));
  const repeated = crypto.randomUUID();
  await native("narration.render", {
    script_id: saved.id,
    job_id: repeated,
    preview: false,
    block_ids: null,
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: repeated })).state, {
      timeout: 180000,
    })
    .toBe("completed");
  expect(await readdir(join(root, "workspace/audio/chunks"))).toEqual(cacheBefore);
  const preview = crypto.randomUUID();
  await native("narration.render", {
    script_id: saved.id,
    job_id: preview,
    preview: true,
    block_ids: [document.content[0]?.attrs.id],
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: preview })).state, {
      timeout: 180000,
    })
    .toBe("completed");
  const previewResult = (await native<JobSnapshot>("jobs.snapshot", { id: preview })).result;
  expect(previewResult?.kind).toBe("render");
  const ambientPath = join(root, "ambience.wav");
  const ambient = Buffer.alloc(44 + 4800 * 2);
  ambient.write("RIFF", 0);
  ambient.writeUInt32LE(ambient.length - 8, 4);
  ambient.write("WAVEfmt ", 8);
  ambient.writeUInt32LE(16, 16);
  ambient.writeUInt16LE(1, 20);
  ambient.writeUInt16LE(1, 22);
  ambient.writeUInt32LE(48000, 24);
  ambient.writeUInt32LE(96000, 28);
  ambient.writeUInt16LE(2, 32);
  ambient.writeUInt16LE(16, 34);
  ambient.write("data", 36);
  ambient.writeUInt32LE(9600, 40);
  for (let i = 0; i < 4800; i++)
    ambient.writeInt16LE(Math.round(Math.sin(i / 10) * 200), 44 + i * 2);
  await writeFile(ambientPath, ambient);
  const imported = await native<{ id: string }>("playback.import_ambience", { path: ambientPath });
  const mixId = crypto.randomUUID();
  const mixedPath = join(root, "mixed.wav");
  await native("narration.export", {
    render_id: render.id,
    path: mixedPath,
    ambience_id: imported.id,
    job_id: mixId,
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: mixId })).state)
    .toBe("completed");
  expect((await readFile(mixedPath)).readUInt16LE(22)).toBe(2);
  const cancelled = crypto.randomUUID();
  await native("generation.start", {
    request: {
      jobId: cancelled,
      scriptId: saved.id,
      baseRevision: saved.revision,
      baseContentHash: saved.contentHash,
      anchor: null,
      range: null,
      modelId: text.id,
      operation: "continue",
      instruction: "Write a long calming story. /no_think",
      selectedText: "",
    },
  });
  await expect
    // Observe process ownership; a fast result may finish between text snapshots.
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: cancelled })).state, {
      timeout: 60000,
      intervals: [100, 250],
    })
    .toBe("running");
  await native("jobs.cancel", { id: cancelled });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: cancelled })).state, {
      timeout: 30000,
    })
    .toBe("cancelled");
  const restarted = crypto.randomUUID();
  await native("models.verify", { id: text.id, job_id: restarted });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: restarted })).state, {
      timeout: 60000,
    })
    .toBe("completed");
  await page.reload();
  await page.getByRole("button", { name: "Native audio fixture", exact: true }).click();
  await page.getByRole("button", { name: "Ask AI", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Suggested passage" })).toBeVisible();
  await page
    .getByRole("textbox", { name: "Script title", exact: true })
    .fill("Native audio revised");
  await expect(page.getByRole("button", { name: "Insert after", exact: true })).toBeDisabled();
  await page.keyboard.press("Control+s");
  await page.getByRole("button", { name: "Discard", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Suggested passage" })).toHaveCount(0);
  await page.getByRole("button", { name: "Narrate", exact: true }).click();
  await expect(page.getByText("Audio from an earlier revision", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Back to library" }).click();
  await native("scripts.bulk", {
    request: { ids: [saved.id], operation: "trash", collectionId: null },
  });
  await native("scripts.bulk", {
    request: { ids: [saved.id], operation: "delete", collectionId: null },
  });
  await native("models.remove", { id: text.id });
  await native("models.remove", { id: speech.id });
  await page.reload();
});

test("creates, saves, reloads, duplicates and recovers real persisted scripts", async () => {
  await page.getByRole("button", { name: "New script", exact: true }).first().click();
  const title = page.getByRole("textbox", { name: "Script title", exact: true });
  await expect(title).toBeFocused();
  await title.fill("A quiet evening");
  await page.getByRole("button", { name: "Background sound", exact: true }).click();
  await page
    .getByRole("dialog", { name: "Background sound" })
    .getByRole("button", { name: "Cancel" })
    .click();
  await expect(title).toHaveValue("A quiet evening");
  await page
    .getByRole("textbox", { name: "Script body", exact: true })
    .fill("Let the day settle, one breath at a time. 🌙");
  await page.keyboard.press("Control+s");
  await expect(page.locator(".editor-status")).toContainText("Saved");
  await page.getByRole("button", { name: "Back to library" }).click();
  await page.getByRole("button", { name: "A quiet evening", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Script body" })).toContainText(
    "one breath at a time. 🌙",
  );
  await page.getByRole("button", { name: "Script actions" }).click();
  await page.getByRole("button", { name: "Duplicate", exact: true }).click();
  await expect(title).toHaveValue("A quiet evening copy");
  await page.getByRole("button", { name: "Back to library" }).click();
  await expect(page.locator("tbody tr")).toHaveCount(2);
  await page.getByRole("checkbox", { name: "Select A quiet evening copy", exact: true }).click();
  const selection = page.getByRole("group", { name: "Selected script actions" });
  await expect(selection).toBeVisible();
  expect((await selection.boundingBox())?.height).toBeLessThan(64);
  await selection.getByRole("button", { name: "Clear", exact: true }).click();
  await page.getByRole("button", { name: "Actions for A quiet evening copy", exact: true }).click();
  await page.getByRole("menuitem", { name: "Rename", exact: true }).click();
  await page.getByRole("textbox", { name: "Script name", exact: true }).fill("Evening copy");
  await page.getByRole("button", { name: "Save name", exact: true }).click();
  await page.getByRole("button", { name: "Actions for Evening copy", exact: true }).click();
  await page.getByRole("menuitem", { name: "Move to Trash", exact: true }).click();
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "Trash", exact: true }).click();
  await page.getByRole("button", { name: "Restore script", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Your trash is empty" })).toBeVisible();
  await page.getByRole("button", { name: "All scripts", exact: true }).click();
  await expect(page.locator("tbody tr")).toHaveCount(2);
});

test("lossless import, batch export and collection deletion preserve scripts", async () => {
  const collection = await native<{ id: string }>("library.create_collection", {
    name: "Synthetic collection",
  });
  await native("library.rename_collection", { id: collection.id, name: "Renamed collection" });
  const script = await native<Script>("scripts.create", { collection_id: collection.id });
  const saved = await native<Script>("scripts.save", {
    patch: {
      id: script.id,
      title: "Round-trip fixture",
      document: {
        version: 2,
        type: "doc",
        content: [
          {
            type: "paragraph",
            attrs: { id: crypto.randomUUID() },
            content: [{ type: "text", text: "Quiet 🌙", marks: [{ type: "bold" }] }],
          },
        ],
      },
      expectedRevision: script.revision,
      collectionId: collection.id,
      favorite: true,
      lastModelId: null,
      session: { ...script.session, paragraphPauseMs: 1250, speed: 0.85 },
    },
  });
  const path = join(root, "script.json");
  await native("scripts.export_file", { id: saved.id, path });
  const imported = await native<Script>("scripts.import_file", {
    path,
    collection_id: collection.id,
  });
  expect(imported.document).toEqual(saved.document);
  expect(imported.session).toEqual(saved.session);
  expect(imported.favorite).toBe(true);
  expect(imported.lastModelId).toBe(saved.lastModelId);
  const exportId = crypto.randomUUID();
  await native("scripts.export_many", {
    ids: [saved.id, imported.id],
    directory: root,
    format: "json",
    job_id: exportId,
  });
  await expect
    .poll(async () => (await native<JobSnapshot>("jobs.snapshot", { id: exportId })).state)
    .toBe("completed");
  const result = (await native<JobSnapshot>("jobs.snapshot", { id: exportId })).result;
  if (result?.kind !== "script-export") throw new Error("Missing batch export result");
  expect((await readdir(result.directory)).length).toBe(2);
  await native("library.delete_collection", { id: collection.id });
  expect((await native<Script>("scripts.get", { id: saved.id })).collectionId).not.toBe(
    collection.id,
  );
  await native("scripts.bulk", {
    request: { ids: [saved.id, imported.id], operation: "trash", collectionId: null },
  });
  await native("scripts.bulk", {
    request: { ids: [saved.id, imported.id], operation: "delete", collectionId: null },
  });
  await page.reload();
  await expect(page.getByRole("heading", { name: "All scripts", exact: true })).toBeVisible();
});

test("dark and light layouts fit the four approved viewports", async () => {
  for (const theme of ["light", "dark"]) {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page
      .getByRole("button", { name: theme === "dark" ? "Dark" : "Light", exact: true })
      .click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await page.getByRole("button", { name: "All scripts", exact: true }).click();
    for (const [width, height] of [
      [1440, 960],
      [1024, 768],
      [768, 1024],
      [390, 844],
    ] as const) {
      await page.setViewportSize({ width, height });
      await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
        .toBe(true);
      await page.screenshot({ path: `test-results/library-${theme}-${width}.png` });
      await page.getByRole("button", { name: "A quiet evening", exact: true }).click();
      await expect(page.getByRole("textbox", { name: "Script body" })).toBeVisible();
      await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
        .toBe(true);
      await page.screenshot({ path: `test-results/write-${theme}-${width}.png` });
      await page.getByRole("button", { name: "Narrate", exact: true }).click();
      await expect(page.locator('[aria-label="Audio player"]')).toHaveCount(1);
      await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
        .toBe(true);
      await page.screenshot({ path: `test-results/narrate-${theme}-${width}.png` });
      await page.getByRole("button", { name: "Back to library" }).click();
    }
    await page.setViewportSize({ width: 1440, height: 960 });
  }
  for (const target of ["Models & voices", "Settings"]) {
    await page.getByRole("button", { name: target, exact: true }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    await expect
      .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
      .toBe(true);
    await page.screenshot({
      path: `test-results/${target === "Settings" ? "settings" : "models"}-dark-390.png`,
    });
    await page.setViewportSize({ width: 1440, height: 960 });
  }
  expect(errors).toEqual([]);
});

test("model and settings controls use themed popups and persist typed values", async () => {
  const model: ModelRecord = {
    id: "synthetic-speech-model",
    name: "Supertonic test voices",
    provider: "nobodywho",
    capability: "speech",
    architecture: "supertonic",
    path: join(root, "synthetic-speech"),
    ownership: "linked",
    status: "ready",
    reason: null,
    fingerprint: "synthetic-ui-fixture",
    contextTokens: null,
    voices: Array.from({ length: 10 }, (_, index) => ({
      id: `synthetic-voice-${index + 1}`,
      name: `F${index + 1}`,
      language: "en",
    })),
  };
  // Seed only this run's disposable database. No model files are opened and no
  // inference, verification or download is started. IPC remains unmodified.
  const database = new DatabaseSync(join(root, "workspace", "hypnofy.sqlite3"));
  try {
    database.exec("PRAGMA busy_timeout=5000");
    database
      .prepare("INSERT INTO installed_models(id,name,record_json) VALUES(?,?,?)")
      .run(model.id, model.name, JSON.stringify(model));
  } finally {
    database.close();
  }
  await page.reload();
  await expect(page.getByRole("heading", { name: "All scripts", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
  expect(await native<ModelRecord[]>("models.list")).toEqual([model]);
  await page.setViewportSize({ width: 1440, height: 960 });
  for (const theme of ["light", "dark"]) {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page
      .getByRole("button", { name: theme === "dark" ? "Dark" : "Light", exact: true })
      .click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await page.getByRole("button", { name: "Models & voices", exact: true }).click();
    await expect(page.getByRole("heading", { name: model.name, exact: true })).toBeVisible();
    const voice = page.getByRole("button", {
      name: `Default voice from ${model.name}`,
      exact: true,
    });
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 960 });
      await voice.click();
      const popup = page.getByRole("listbox");
      await expect(popup).toBeVisible();
      const box = await popup.boundingBox();
      if (!box) throw new Error("Missing model voice popup");
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      expect(box.y + box.height).toBeLessThanOrEqual(960);
      await page.screenshot({ path: `test-results/model-controls-${theme}-${width}.png` });
      await page.getByRole("option", { name: "F4 · en", exact: true }).click();
      await expect(voice).toContainText("F4 · en");
      await expect
        .poll(async () => (await native<Preferences>("settings.get")).voiceId)
        .toBe("synthetic-voice-4");
      expect(await page.locator("select:visible").count()).toBe(0);
    }
    await page.setViewportSize({ width: 1440, height: 960 });
  }
  await page.getByRole("button", { name: "Link existing", exact: true }).click();
  await page.getByRole("button", { name: "Model type", exact: true }).click();
  await page.getByRole("option", { name: "Supertonic · speech folder", exact: true }).click();
  await expect(page.getByRole("button", { name: "Choose folder", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Voice & playback", exact: true }).click();
  const pace = page.getByRole("spinbutton", { name: "Default pace", exact: true });
  await pace.fill("1.15");
  await pace.press("Tab");
  await expect.poll(async () => (await native<Preferences>("settings.get")).speed).toBe(1.15);
  const volume = page.getByRole("slider", { name: "Default narration volume", exact: true });
  await volume.focus();
  await volume.press("Home");
  await volume.press("ArrowRight");
  await expect
    .poll(async () => (await native<Preferences>("settings.get")).narrationVolume)
    .toBe(0.01);
  await page.screenshot({ path: "test-results/settings-controls-dark.png" });
  expect(errors).toEqual([]);
});

test("AI review and delivery stay usable while written pause cues compile to silence", async () => {
  test.setTimeout(90000);
  await page.getByRole("button", { name: "New script", exact: true }).first().click();
  await page.getByRole("textbox", { name: "Script title", exact: true }).fill("Workspace fixture");
  await page
    .getByRole("textbox", { name: "Script body" })
    .fill("Take a breath.\npause 5 seconds\nRest.");
  await page.keyboard.press("Control+s");
  const summaries = await native<ScriptSummary[]>("library.scripts", {
    query: { collectionId: null, query: "Workspace fixture", favorite: false, trash: false },
  });
  const summary = summaries[0];
  if (!summary) throw Error("Missing workspace fixture");
  const script = await native<Script>("scripts.get", { id: summary.id });
  const plan = await native<{ kind: string; text?: string; durationMs?: number }[]>(
    "narration.plan",
    { script_id: script.id },
  );
  expect(plan.filter((u) => u.kind === "silence").map((u) => u.durationMs)).toContain(5000);
  expect(
    plan
      .filter((u) => u.kind === "speech")
      .map((u) => u.text)
      .join(" "),
  ).not.toContain("pause 5 seconds");
  const database = new DatabaseSync(join(root, "workspace", "hypnofy.sqlite3"));
  const draftText =
    Array.from(
      { length: 60 },
      (_, i) =>
        `Passage ${i + 1}. Let this moment arrive in its own time. There is nothing you need to hurry.`,
    ).join("\n\n") + "\n\n[pause:3s]\n\nReturn gently.";
  try {
    database.prepare("INSERT INTO suggestions(id,script_id,record_json) VALUES(?,?,?)").run(
      "ui-draft",
      script.id,
      JSON.stringify({
        id: "ui-draft",
        scriptId: script.id,
        baseRevision: script.revision,
        baseContentHash: script.contentHash,
        text: draftText,
        modelId: "synthetic-text-model",
        anchor: null,
        range: null,
        operation: "continue",
      }),
    );
  } finally {
    database.close();
  }
  await page.reload();
  await page.getByRole("button", { name: "Workspace fixture", exact: true }).click();
  await page.getByRole("button", { name: "Open AI panel", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Write with AI", exact: true })).toBeVisible();
  await expect(page.locator(".inspector")).toHaveCount(0);
  for (const theme of ["dark", "light"])
    for (const [width, height] of [
      [1440, 960],
      [1024, 768],
      [768, 1024],
      [390, 844],
    ] as const) {
      await page.setViewportSize({ width, height });
      await page.evaluate((theme) => (document.documentElement.dataset.theme = theme), theme);
      if (width < 768) await page.getByRole("button", { name: "Preview", exact: true }).click();
      const actions = page.getByRole("button", { name: "Insert after", exact: true });
      await expect(actions).toBeInViewport();
      const reading = page.locator(".draft-reading");
      expect(await reading.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
      await reading.evaluate((el) => (el.scrollTop = el.scrollHeight));
      await expect(actions).toBeInViewport();
      expect(
        await page.locator(".ai-modal").evaluate((el) => el.scrollWidth <= el.clientWidth),
      ).toBe(true);
      await page.screenshot({ path: `test-results/ai-workspace-${theme}-${width}.png` });
    }
  await page.getByRole("button", { name: "Insert after", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.keyboard.press("Control+s");
  await expect
    .poll(async () =>
      JSON.stringify((await native<Script>("scripts.get", { id: script.id })).document),
    )
    .toContain('"inlinePause"');
  await page.getByRole("button", { name: "Open delivery panel", exact: true }).click();
  await page.getByRole("button", { name: "Whole script", exact: true }).click();
  await page.getByRole("button", { name: "Slow", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Script pace", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Done", exact: true })).toBeInViewport();
  await page.screenshot({ path: "test-results/delivery-v2-light-390.png" });
  await page.getByText("More timing options", { exact: false }).click();
  await expect(page.getByRole("slider", { name: "Script pace", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.keyboard.press("Control+s");
  await expect
    .poll(async () => (await native<Script>("scripts.get", { id: script.id })).session.speed)
    .toBe(0.8);
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
  expect(errors).toEqual([]);
});

test("sound library imports, previews, renames and assigns background audio", async () => {
  const frames = 48000 * 6;
  const wave = Buffer.alloc(44 + frames * 2);
  wave.write("RIFF", 0);
  wave.writeUInt32LE(wave.length - 8, 4);
  wave.write("WAVEfmt ", 8);
  wave.writeUInt32LE(16, 16);
  wave.writeUInt16LE(1, 20);
  wave.writeUInt16LE(1, 22);
  wave.writeUInt32LE(48000, 24);
  wave.writeUInt32LE(96000, 28);
  wave.writeUInt16LE(2, 32);
  wave.writeUInt16LE(16, 34);
  wave.write("data", 36);
  wave.writeUInt32LE(frames * 2, 40);
  const source = join(root, "Synthetic calm.wav");
  await writeFile(source, wave);
  const sound = await native<Ambience>("playback.import_ambience", { path: source });
  expect(sound.path).not.toBe(source);
  await page.getByRole("button", { name: "Sound library", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Sound library", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Preview Synthetic calm", exact: true }).click();
  await expect
    .poll(async () => (await native<PlaybackState>("playback.state")).previewAmbienceId)
    .toBe(sound.id);
  await page.getByRole("button", { name: "Stop preview Synthetic calm", exact: true }).click();
  await expect
    .poll(async () => (await native<PlaybackState>("playback.state")).previewAmbienceId)
    .toBeNull();
  await page.getByRole("button", { name: "Rename Synthetic calm", exact: true }).click();
  await page.getByRole("textbox", { name: "Sound name", exact: true }).fill("Evening piano");
  await page.getByRole("button", { name: "Save name", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Evening piano", exact: true })).toBeVisible();
  for (const [width, height] of [
    [1440, 960],
    [390, 844],
  ] as const) {
    await page.setViewportSize({ width, height });
    await expect
      .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
      .toBe(true);
    await page.screenshot({ path: `test-results/sound-library-dark-${width}.png` });
  }
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.getByRole("button", { name: "All scripts", exact: true }).click();
  await page.getByRole("button", { name: "Workspace fixture", exact: true }).click();
  await page.getByRole("button", { name: "Background sound", exact: true }).click();
  const background = page.getByRole("dialog", { name: "Background sound" });
  await background.getByRole("button", { name: "Sound for this script" }).click();
  await page.getByRole("option", { name: "Evening piano", exact: true }).click();
  await background.getByRole("button", { name: "Save sound", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Script body" })).toBeVisible();
  const summaries = await native<ScriptSummary[]>("library.scripts", {
    query: { collectionId: null, query: "Workspace fixture", favorite: false, trash: false },
  });
  const id = summaries[0]?.id;
  if (!id) throw Error("Missing sound script");
  await expect
    .poll(async () => (await native<Script>("scripts.get", { id })).session.ambienceId)
    .toBe(sound.id);
  expect((await readFile(source)).length).toBe(wave.length);
  await page.getByRole("button", { name: "Narrate", exact: true }).click();
  await page.getByRole("button", { name: "Manage recordings", exact: true }).click();
  await page.getByRole("button", { name: "Back to script", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Workspace fixture", exact: true })).toBeVisible();
  await page.setViewportSize({ width: 1440, height: 960 });
  const saved = await native<Script>("scripts.get", { id });
  const legacyRenderId = crypto.randomUUID();
  const legacyRecord: RenderRecord = {
    id: legacyRenderId,
    scriptId: id,
    revision: saved.revision,
    contentHash: saved.contentHash,
    durationMs: 6000,
    createdAt: new Date().toISOString(),
    preview: false,
    pipelineVersion: 0,
    passages: [],
  };
  await writeFile(join(root, "workspace", "audio", "renders", `${legacyRenderId}.wav`), wave);
  const database = new DatabaseSync(join(root, "workspace", "hypnofy.sqlite3"));
  try {
    database
      .prepare("INSERT INTO render_records(id,script_id,record_json) VALUES(?,?,?)")
      .run(legacyRenderId, id, JSON.stringify(legacyRecord));
  } finally {
    database.close();
  }
  await page.reload();
  await page.getByRole("button", { name: "Workspace fixture", exact: true }).click();
  await expect(
    page.getByText("This audio uses the older pause pipeline.", { exact: false }),
  ).toBeVisible();
  await native("playback.load", { render_id: legacyRenderId });
  await native("playback.control", { operation: "play", value: null });
  await expect.poll(async () => (await native<PlaybackState>("playback.state")).playing).toBe(true);
  await native("playback.control", { operation: "stop", value: null });
  expect(errors).toEqual([]);
});

test("public model browsing includes unsupported architectures, cards, files and pagination", async () => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.getByRole("button", { name: "Models & voices", exact: true }).click();
  await page.getByRole("button", { name: "Explore Hugging Face", exact: true }).click();
  await page.getByRole("button", { name: "Qwen3-TTS", exact: true }).click();
  const model = page.getByRole("button", {
    name: "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
    exact: true,
  });
  await expect(model).toBeVisible();
  await expect
    .poll(() =>
      hub.requests.some(
        (path) => path.includes("search=Qwen3-TTS") && path.includes("pipeline_tag=text-to-speech"),
      ),
    )
    .toBe(true);
  await model.click();
  const dialog = page.getByRole("dialog", { name: "Model details" });
  await expect(dialog.getByText("Synthetic voice model card for browsing tests.")).toBeVisible();
  await expect(dialog.getByText("Browse only", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Review supported download" })).toHaveCount(0);
  await expect(dialog.getByRole("link", { name: "Full model page", exact: false })).toHaveAttribute(
    "href",
    "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
  );
  await page.screenshot({ path: "test-results/model-details-dark-1440.png" });
  // Test profiles validate browser requests without opening a user's browser.
  await dialog.getByRole("link", { name: "Full model page", exact: false }).click();
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  await expect(
    native("models.open_source", { url: "file:///C:/Windows/notepad.exe" }),
  ).rejects.toBeDefined();
  await dialog.getByRole("button", { name: "Files (2)" }).click();
  await dialog.getByRole("textbox", { name: "Filter model files" }).fill("safetensors");
  await expect(dialog.getByRole("link", { name: "model.safetensors", exact: false })).toContainText(
    "3.46 GB",
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(dialog.getByRole("button", { name: "Done", exact: true })).toBeInViewport();
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  await page.screenshot({ path: "test-results/model-details-dark-390.png" });
  await dialog.getByRole("button", { name: "Close model details" }).click();
  await page.getByRole("button", { name: "Load more models" }).click();
  await expect(
    page.getByRole("button", { name: "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign", exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: "test-results/model-explorer-dark-390.png" });
  await page.setViewportSize({ width: 1440, height: 960 });
  await expect(page.locator(".hub-model")).toHaveCount(3);
  await page.getByRole("button", { name: "Model task", exact: true }).click();
  await page.getByRole("option", { name: "All model tasks", exact: true }).click();
  await page.getByRole("button", { name: "Sort models", exact: true }).click();
  await page.getByRole("option", { name: "Recently updated", exact: true }).click();
  await expect
    .poll(() =>
      hub.requests.some(
        (path) => path.includes("sort=lastModified") && !path.includes("pipeline_tag"),
      ),
    )
    .toBe(true);
  await expect(page.locator(".hub-model")).toHaveCount(2);
  await page.screenshot({ path: "test-results/model-explorer-dark-1440.png" });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Light", exact: true }).click();
  await page.getByRole("button", { name: "Models & voices", exact: true }).click();
  await page.getByRole("button", { name: "Explore Hugging Face", exact: true }).click();
  await model.click();
  await expect(dialog.getByText("Synthetic voice model card for browsing tests.")).toBeVisible();
  await page.screenshot({ path: "test-results/model-details-light-1440.png" });
  await dialog.getByRole("button", { name: "Close model details" }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Dark", exact: true }).click();
  expect(errors).toEqual([]);
});

test("trashing unloads audio, recording deletion removes owned files, and exports survive", async () => {
  const created = await native<Script>("scripts.create", { collection_id: null });
  const script = await native<Script>("scripts.save", {
    patch: {
      id: created.id,
      title: "Recording ownership",
      document: created.document,
      lastModelId: null,
      collectionId: created.collectionId,
      favorite: false,
      expectedRevision: created.revision,
      session: created.session,
    },
  });
  const wave = await readFile(join(root, "Synthetic calm.wav"));
  const record: RenderRecord = {
    id: crypto.randomUUID(),
    scriptId: script.id,
    revision: script.revision,
    contentHash: script.contentHash,
    durationMs: 6000,
    createdAt: new Date().toISOString(),
    preview: false,
    pipelineVersion: 2,
    passages: [],
  };
  const path = join(root, "workspace", "audio", "renders", `${record.id}.wav`);
  const manifest = path.replace(/\.wav$/, ".json");
  await writeFile(path, wave);
  await writeFile(manifest, JSON.stringify({ units: [] }));
  const exported = join(root, "user-export.wav");
  await writeFile(exported, wave);
  const database = new DatabaseSync(join(root, "workspace", "hypnofy.sqlite3"));
  try {
    database
      .prepare("INSERT INTO render_records(id,script_id,record_json) VALUES(?,?,?)")
      .run(record.id, script.id, JSON.stringify(record));
  } finally {
    database.close();
  }
  await native("playback.load", { render_id: record.id });
  await native("playback.control", { operation: "play", value: null });
  await native("scripts.bulk", {
    request: { ids: [script.id], operation: "trash", collectionId: null },
  });
  expect(await native<PlaybackState>("playback.state")).toMatchObject({
    renderId: null,
    playing: false,
    durationMs: 0,
  });
  expect((await readFile(path)).length).toBe(wave.length);
  expect(
    (await native<StoredRecording[]>("narration.library")).find((r) => r.render.id === record.id)
      ?.scriptStatus,
  ).toBe("trash");
  await page.getByRole("button", { name: "Sound library", exact: true }).click();
  await page.getByRole("button", { name: "Recordings", exact: true }).click();
  const row = page.locator(".recording-row").filter({ hasText: "Recording ownership" });
  await expect(row.getByText("In Trash", { exact: true })).toBeVisible();
  await expect(
    row.getByRole("button", { name: "Play recording Recording ownership" }),
  ).toBeDisabled();
  await row.getByRole("button", { name: "Restore script" }).click();
  await row.getByRole("button", { name: "Play recording Recording ownership" }).click();
  await expect
    .poll(async () => (await native<PlaybackState>("playback.state")).renderId)
    .toBe(record.id);
  await page.getByRole("button", { name: "Close player", exact: true }).click();
  await expect
    .poll(async () => (await native<PlaybackState>("playback.state")).renderId)
    .toBeNull();
  await page.setViewportSize({ width: 390, height: 844 });
  await row.getByRole("button", { name: "Play recording Recording ownership" }).click();
  await expect(
    row.getByRole("button", { name: "Delete recording Recording ownership" }),
  ).toBeVisible();
  await page.screenshot({ path: "test-results/recordings-dark-390.png" });
  await row.getByRole("button", { name: "Delete recording Recording ownership" }).click();
  await page
    .getByRole("dialog", { name: "Delete recording?" })
    .getByRole("button", { name: "Delete recording", exact: true })
    .click();
  await expect(row).toHaveCount(0);
  await expect
    .poll(async () => (await native<PlaybackState>("playback.state")).renderId)
    .toBeNull();
  await expect
    .poll(() =>
      readdir(join(root, "workspace", "audio", "renders")).then((files) =>
        files.includes(`${record.id}.wav`),
      ),
    )
    .toBe(false);
  expect(await native<Script>("scripts.get", { id: script.id })).toMatchObject({
    title: "Recording ownership",
    deletedAt: null,
  });
  expect(await readFile(exported)).toEqual(wave);
  expect(
    (await readdir(join(root, "workspace", "audio", "renders"))).includes(`${record.id}.json`),
  ).toBe(false);
  await page.setViewportSize({ width: 1440, height: 960 });
  expect(errors).toEqual([]);
});

test("native window resizing fills the client area and preserves the editor", async () => {
  test.setTimeout(90000);
  // Screenshot emulation persists in its CDP session. Native bounds need a fresh
  // process that has never received page.setViewportSize().
  await stopApp();
  await startApp();
  expect(page.viewportSize()).toBeNull();
  await page.getByRole("checkbox", { name: "I have read the welcome notice" }).click();
  await page.getByRole("button", { name: "Accept & continue" }).click();
  await page.getByRole("button", { name: "New script", exact: true }).first().click();
  await page
    .getByRole("textbox", { name: "Script title", exact: true })
    .fill("Native resize fixture");
  await page
    .getByRole("textbox", { name: "Script body" })
    .fill("Let the day settle, one breath at a time.");
  await page.keyboard.press("Control+s");
  await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke("plugin:window|unmaximize", { label: "main" }),
  );
  for (const [width, height] of [
    [900, 700],
    [1600, 1000],
    [1024, 768],
    [390, 844],
    [1440, 960],
  ] as const) {
    await page.evaluate(
      ({ width, height }) =>
        window.__TAURI_INTERNALS__.invoke("plugin:window|set_size", {
          label: "main",
          value: { Logical: { width, height } },
        }),
      { width, height },
    );
    await expect
      .poll(() => page.evaluate(() => ({ width: innerWidth, height: innerHeight })))
      .toEqual({ width, height });
    const bounds = await page.locator(".app-shell").boundingBox();
    expect(bounds?.width).toBe(width);
    expect(bounds?.height).toBe(height);
    await expect(page.getByRole("textbox", { name: "Script body" })).toContainText(
      "one breath at a time",
    );
    await expect(page.locator(".editor-status")).toBeInViewport();
    expect(
      await page
        .locator(".writing-scroll")
        .evaluate((el) => getComputedStyle(el, "::-webkit-scrollbar").width),
    ).toBe("10px");
  }
  await page.screenshot({ path: "test-results/native-resized-editor.png" });
  await page.evaluate(async () => {
    await window.__TAURI_INTERNALS__.invoke("plugin:window|maximize", { label: "main" });
    await window.__TAURI_INTERNALS__.invoke("plugin:window|hide", { label: "main" });
  });
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const size = await window.__TAURI_INTERNALS__.invoke<{ width: number; height: number }>(
          "plugin:window|inner_size",
          { label: "main" },
        );
        const factor = await window.__TAURI_INTERNALS__.invoke<number>(
          "plugin:window|scale_factor",
          { label: "main" },
        );
        const root = document.querySelector(".app-shell")?.getBoundingClientRect();
        return {
          // WebView layout can have fractional CSS pixels at 150% DPI. Compare
          // physical pixels before applying the one-pixel platform tolerance.
          fits:
            !!root &&
            Math.abs(Math.round(root.width * factor) - size.width) <= 1 &&
            Math.abs(Math.round(root.height * factor) - size.height) <= 1,
          native: size,
          factor,
          root: root ? { width: root.width, height: root.height } : null,
          viewport: { width: innerWidth, height: innerHeight, pixelRatio: devicePixelRatio },
        };
      }),
    )
    .toMatchObject({ fits: true });
});
