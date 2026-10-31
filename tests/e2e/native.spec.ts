import { test, expect, chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtemp, rm, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { ChildProcess } from "node:child_process";
import type { Browser, Page } from "@playwright/test";
import type {
  ModelRecord,
  JobSnapshot,
  Script,
  ScriptPatch,
  RenderRecord,
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
test.describe.configure({ mode: "serial" });

async function native<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  return page.evaluate(
    ({ name, args }) => window.__TAURI_INTERNALS__.invoke<T>(`TauRPC__${name}`, args),
    { name, args },
  );
}

async function startApp(): Promise<void> {
  nativeDiagnostics = "";
  root = await mkdtemp(join(tmpdir(), "hypnofy-e2e-"));
  const port = 19100 + Math.floor(Math.random() * 1000);
  process = spawn(resolve(".b/debug/Hypnofy-dev.exe"), [], {
    windowsHide: true,
    env: {
      ...globalThis.process.env,
      HYPNOFY_TEST_DATA_DIR: join(root, "workspace"),
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

test.afterAll(stopApp);

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
  await title.fill("A quiet evening");
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
  await page.getByRole("button", { name: "Move script to Trash" }).first().click();
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
        return (
          !!root &&
          Math.abs(root.width * factor - size.width) <= 1 &&
          Math.abs(root.height * factor - size.height) <= 1
        );
      }),
    )
    .toBe(true);
});
