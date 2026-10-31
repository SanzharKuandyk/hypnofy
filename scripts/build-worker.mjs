import { execFile, spawn } from "node:child_process";
import { access, readdir } from "node:fs/promises";
import { delimiter, dirname, join, resolve } from "node:path";
import { promisify } from "node:util";

async function isSdk(path) {
  try {
    await Promise.all([
      access(join(path, "Include/vulkan/vulkan.h")),
      access(join(path, "Bin/glslc.exe")),
      access(join(path, "Lib/vulkan-1.lib")),
    ]);
    return true;
  } catch {
    return false;
  }
}

async function sdkDirectory() {
  const configured = process.env.VULKAN_SDK;
  if (configured && (await isSdk(configured))) return configured;
  const root = "C:/VulkanSDK";
  const versions = await readdir(root, { withFileTypes: true }).catch(() => []);
  const candidates = versions
    .filter((entry) => entry.isDirectory() && /^\d+(\.\d+)+$/.test(entry.name))
    .map((entry) => entry.name)
    .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  for (const version of candidates) {
    const path = join(root, version);
    if (await isSdk(path)) return path;
  }
  throw new Error("Install the Vulkan SDK and set VULKAN_SDK to its directory.");
}

if (process.platform !== "win32") {
  throw new Error("The native integration gate currently targets Windows x64.");
}

const sdk = await sdkDirectory();
const ninja = "C:/DevTools/ninja.exe";
await access(ninja);
const execute = promisify(execFile);
const { stdout: installation } = await execute(
  "C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe",
  [
    "-latest",
    "-products",
    "*",
    "-requires",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "-property",
    "installationPath",
  ],
  { windowsHide: true },
);
const developerCommand = join(installation.trim(), "Common7/Tools/VsDevCmd.bat");
await access(developerCommand);
const lint = process.argv.includes("--lint");
const workspace = process.argv.includes("--workspace");
const test = process.argv.includes("--test");
const docs = process.argv.includes("--doc");
const launcher = resolve("scripts/worker-msvc.cmd");
const manifest = resolve("src-tauri/Cargo.toml");
for (const path of [launcher, manifest]) {
  if (/["%\r\n]/.test(path)) throw new Error("Unsupported characters in build path");
}
const operation = lint ? "clippy" : test ? "test" : docs ? "doc" : "build";
const selection = workspace
  ? "--workspace"
  : docs
    ? "-p hypnofy-inference-protocol"
    : "-p hypnofy-inference-worker";
const command = `""${launcher}" ${operation} --locked --manifest-path "${manifest}" ${selection}${lint ? " --all-targets --all-features -- -D warnings" : docs ? " --no-deps" : ""}"`;
const environment = {
  ...process.env,
  VULKAN_SDK: sdk,
  HYPNOFY_VSDEVCMD: developerCommand,
  CMAKE_GENERATOR: "Ninja",
  CMAKE_BUILD_PARALLEL_LEVEL: "4",
  CARGO_BUILD_JOBS: "4",
  // Keep Ninja output separate from existing Visual Studio CMake caches.
  // Nested Vulkan compiler probes exceed MSVC's path limit in target/ninja.
  CARGO_TARGET_DIR: resolve(".b"),
  PATH: `${dirname(ninja)}${delimiter}${process.env.PATH ?? ""}`,
};
delete environment.CMAKE_GENERATOR_PLATFORM;
delete environment.CMAKE_GENERATOR_TOOLSET;
delete environment.CMAKE_GENERATOR_INSTANCE;

const child = spawn("cmd.exe", ["/d", "/s", "/c", command], {
  windowsVerbatimArguments: true,
  windowsHide: true,
  stdio: "inherit",
  env: environment,
});

child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});

child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
