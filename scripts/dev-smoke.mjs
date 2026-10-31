import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "@playwright/test";

const root = await mkdtemp(join(tmpdir(), "hypnofy-dev-smoke-"));
const port = 21000 + Math.floor(Math.random() * 1000);
const child = spawn(process.execPath, ["scripts/tauri.mjs", "dev", "--no-watch"], {
  windowsHide: true,
  env: {
    ...process.env,
    HYPNOFY_TEST_DATA_DIR: join(root, "workspace"),
    WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let diagnostics = "";
child.stdout.on("data", (data) => {
  diagnostics = (diagnostics + data).slice(-8000);
});
child.stderr.on("data", (data) => {
  diagnostics = (diagnostics + data).slice(-8000);
});
let browser;
try {
  const deadline = Date.now() + 240000;
  while (
    !(await fetch(`http://127.0.0.1:${port}/json/version`)
      .then((r) => r.ok)
      .catch(() => false))
  ) {
    if (child.exitCode !== null || Date.now() > deadline)
      throw new Error(`Development startup failed:
${diagnostics}`);
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const context = browser.contexts()[0];
  if (!context) throw new Error("Missing development WebView");
  const page = context.pages()[0] ?? (await context.waitForEvent("page"));
  page.setDefaultTimeout(120000);
  await page.waitForFunction(() => typeof window.__TAURI_INTERNALS__?.invoke === "function");
  await page.getByRole("heading", { name: "Before you begin" }).waitFor();
  await page.getByRole("checkbox", { name: "I have read the welcome notice" }).click();
  await page.getByRole("button", { name: "Accept & continue" }).click();
  await page.getByRole("heading", { name: "All scripts", exact: true }).waitFor();
  await page.goto("http://localhost:1420/?showcase");
  await page.getByRole("heading", { name: "Hypnofy components" }).waitFor();
  await page
    .evaluate(() => window.__TAURI_INTERNALS__.invoke("TauRPC__entry.exit", {}))
    .catch(() => undefined);
  console.log("PASS: npm run tauri dev, new UI, launch notice and development component showcase");
} catch (error) {
  console.error(diagnostics);
  for (const context of browser?.contexts() ?? [])
    for (const page of context.pages())
      console.error(
        page.url(),
        await page
          .locator("body")
          .innerText()
          .catch(() => ""),
      );
  throw error;
} finally {
  await browser?.close().catch(() => undefined);
  if (child.exitCode === null)
    spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
      windowsHide: true,
      stdio: "ignore",
    });
  await new Promise((resolve) => setTimeout(resolve, 500));
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
}
