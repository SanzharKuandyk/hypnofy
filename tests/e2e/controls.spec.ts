import { test, expect, chromium } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { resolve } from "node:path";
import type { ChildProcess } from "node:child_process";
import type { Browser, Page } from "@playwright/test";

let server: ChildProcess;
let browser: Browser;
let page: Page;
const errors: string[] = [];
test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  const port = 23200 + Math.floor(Math.random() * 500);
  server = spawn(
    globalThis.process.execPath,
    [
      resolve("node_modules/vite/bin/vite.js"),
      "--host",
      "127.0.0.1",
      "--port",
      String(port),
      "--strictPort",
    ],
    { windowsHide: true, stdio: "ignore" },
  );
  await expect
    .poll(
      () =>
        fetch(`http://127.0.0.1:${port}`)
          .then((r) => r.ok)
          .catch(() => false),
      { timeout: 30000 },
    )
    .toBe(true);
  browser = await chromium.launch({
    channel: "msedge",
    headless: true,
    // Playwright hides scrollbars in headless mode unless this default is removed.
    ignoreDefaultArgs: ["--hide-scrollbars"],
  });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${port}/?showcase`);
  await expect(page.getByRole("heading", { name: "Hypnofy components" })).toBeVisible({
    timeout: 30000,
  });
});

test.afterAll(async () => {
  await browser?.close();
  if (server?.pid && server.exitCode === null)
    execFileSync("taskkill", ["/PID", String(server.pid), "/T", "/F"], {
      windowsHide: true,
      stdio: "ignore",
    });
});

test("custom selectors scroll, skip disabled options, restore focus and preserve boolean values", async () => {
  const voice = page.getByRole("button", { name: "Preview voice", exact: true });
  await expect(voice).toContainText("F4 · English");
  await expect(page.getByRole("button", { name: "Disabled voice", exact: true })).toBeDisabled();
  await voice.click();
  await expect(page.getByRole("listbox")).toBeVisible();
  const scroller = page.locator(".control-options");
  expect(await scroller.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  expect(await scroller.evaluate((el) => getComputedStyle(el, "::-webkit-scrollbar").display)).toBe(
    "block",
  );
  expect(await scroller.evaluate((el) => getComputedStyle(el, "::-webkit-scrollbar").width)).toBe(
    "10px",
  );
  await expect(page.getByRole("option", { name: "F2 · English", exact: true })).toBeDisabled();
  await scroller.hover();
  await page.mouse.wheel(0, 500);
  await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  const bounds = await scroller.boundingBox();
  const metrics = await scroller.evaluate((el) => ({
    top: el.scrollTop,
    height: el.clientHeight,
    full: el.scrollHeight,
  }));
  if (!bounds) throw new Error("Missing scrollable voice list");
  const thumbHeight = Math.max(40, metrics.height ** 2 / metrics.full);
  const thumbTop = (metrics.top / (metrics.full - metrics.height)) * (metrics.height - thumbHeight);
  await page.mouse.move(bounds.x + bounds.width - 5, bounds.y + thumbTop + thumbHeight / 2);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width - 5, bounds.y + bounds.height - thumbHeight / 2, {
    steps: 5,
  });
  await page.mouse.up();
  await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(metrics.top);
  await expect(voice).toBeFocused();
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(voice).toContainText("F3 · English");
  await voice.click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(voice).toContainText("F40 · English");
  await expect(voice).toBeFocused();
  await voice.press("Space");
  await expect(page.getByRole("listbox")).toBeVisible();
  await expect(page.getByRole("option", { name: "F40 · English", exact: true })).toBeInViewport();
  await page.keyboard.press("Escape");
  await expect(voice).toBeFocused();
  const format = page.getByRole("button", { name: "Preview export", exact: true });
  await format.click();
  await page.getByRole("option", { name: "Narration + ambience", exact: true }).click();
  await expect(format).toContainText("Narration + ambience");
  await format.click();
  await page.getByRole("option", { name: "Narration only", exact: true }).click();
  await expect(format).toContainText("Narration only");
  expect(await page.locator("select:visible").count()).toBe(0);
});

test("steppers validate bounds, sliders support the keyboard, checkboxes and tooltips use app styling", async () => {
  const pace = page.getByRole("spinbutton", { name: "Preview pace", exact: true });
  await pace.fill("1.95");
  await pace.press("Enter");
  await page.getByRole("button", { name: "Increase Preview pace" }).click();
  await expect(pace).toHaveValue("2");
  await expect(page.getByRole("button", { name: "Increase Preview pace" })).toBeDisabled();
  await pace.fill("99");
  await expect(pace).toHaveAttribute("aria-invalid", "true");
  await pace.press("Tab");
  await expect(pace).toHaveValue("2");
  const volume = page.getByRole("slider", { name: "Preview volume", exact: true });
  await volume.focus();
  await page.keyboard.press("ArrowRight");
  await expect(volume).toHaveAttribute("aria-valuenow", "0.61");
  await page.keyboard.press("Home");
  await expect(volume).toHaveAttribute("aria-valuenow", "0");
  const checkbox = page.getByRole("checkbox", { name: "Preview selection", exact: true });
  await checkbox.click();
  await expect(checkbox).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "Mixed selection" })).toHaveAttribute(
    "aria-checked",
    "mixed",
  );
  await expect(page.getByRole("checkbox", { name: "Disabled selection" })).toBeDisabled();
  const help = page.getByRole("button", { name: "Preview tooltip", exact: true });
  await help.hover();
  await expect(page.getByRole("tooltip")).toContainText("Preview tooltip");
  await expect(help).not.toHaveAttribute("title");
  await page.keyboard.press("Escape");
});

test("a portaled selector works inside a dialog and Escape closes only the top layer", async () => {
  await page.getByRole("button", { name: "Open dialog", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await page.getByRole("button", { name: "Dialog voice", exact: true }).click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(dialog).toHaveCount(0);
});

test("dark and light popups fit desktop and narrow windows", async () => {
  for (const theme of ["dark", "light"])
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 1000 });
      await page.evaluate((theme) => {
        document.documentElement.dataset.theme = theme;
        document.querySelector(".page-scroll")?.scrollTo(0, 0);
      }, theme);
      await page.getByRole("button", { name: "Preview voice", exact: true }).click();
      const box = await page.getByRole("listbox").boundingBox();
      if (!box) throw Error("Missing popup");
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      expect(box.y + box.height).toBeLessThanOrEqual(1000);
      await page.screenshot({ path: `test-results/controls-${theme}-${width}.png` });
      await page.keyboard.press("Escape");
    }
  expect(errors).toEqual([]);
});
