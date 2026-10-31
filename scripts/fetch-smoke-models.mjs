import {
  mkdir,
  writeFile,
  rename,
  rm,
  readFile,
  readdir,
  realpath,
  lstat,
  unlink,
  rmdir,
} from "node:fs/promises";
import { createReadStream, createWriteStream } from "node:fs";
import { createHash } from "node:crypto";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { resolve, dirname, join } from "node:path";

// Explicit integration fixtures, isolated from application/user model folders.
const root = resolve(".b/smoke-models");
const marker = "Owned integration fixtures; safe to remove after testing.\n";
const files = [
  [
    "NobodyWho/Qwen_Qwen3-0.6B-GGUF",
    "8cb2bae78af48e5727740bd865d01ce291ea6d33",
    "Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    "text/model.gguf",
    "168a5fd6091254189e2594764db1c66dd6b2984f0091f92a86f388bddd1d9bb2",
  ],
  [
    "NobodyWho/Kokoro-82M",
    "0c5985572257cf2f1bce418b2cf47ed5a9580b14",
    "model.onnx",
    "speech/model.onnx",
    "a287341900ae88fd8c049af4560a6d270aecba6b8859f59c307ba50dd0d0afb6",
  ],
  [
    "NobodyWho/Kokoro-82M",
    "0c5985572257cf2f1bce418b2cf47ed5a9580b14",
    "voices/bf_emma.safetensors",
    "speech/voices/bf_emma.safetensors",
    "2145949882a4bf36af1b2f7254a068ec765fc5e731ad7e6f2db5ecd1029d09de",
  ],
  [
    "NobodyWho/Kokoro-82M",
    "0c5985572257cf2f1bce418b2cf47ed5a9580b14",
    "config.json",
    "speech/config.json",
    null,
  ],
];

// Only remove this downloader's exact, verified fixture set. Never recurse or
// accept a user-selected path; unknown files and links cause a full refusal.
if (process.argv.includes("--clean")) {
  const base = await realpath(resolve(".b"));
  if ((await realpath(root)) !== join(base, "smoke-models")) throw Error("Fixture root changed");
  if ((await readFile(join(root, ".hypnofy-test-fixtures"), "utf8")) !== marker)
    throw Error("Fixture ownership marker is missing or changed");
  const expected = new Map([
    ["", [".hypnofy-test-fixtures", "speech", "text"]],
    ["text", ["model.gguf"]],
    ["speech", ["config.json", "model.onnx", "voices"]],
    ["speech/voices", ["bf_emma.safetensors"]],
  ]);
  for (const [directory, names] of expected) {
    const entries = await readdir(join(root, directory));
    if (entries.sort().join("\n") !== names.sort().join("\n"))
      throw Error("Unknown fixture files were preserved");
    for (const name of entries) {
      const path = join(root, directory, name);
      if (
        (await lstat(path)).isSymbolicLink() ||
        (!(await realpath(path)).startsWith(root + "/") &&
          !(await realpath(path)).startsWith(root + "\\"))
      )
        throw Error("Fixture links were preserved");
    }
  }
  for (const [, , , destination, expectedHash] of files) {
    if (!expectedHash) {
      if ((await lstat(join(root, destination))).size !== 2351)
        throw Error("Fixture config changed");
      continue;
    }
    const hash = createHash("sha256");
    for await (const chunk of createReadStream(join(root, destination))) hash.update(chunk);
    if (hash.digest("hex") !== expectedHash)
      throw Error("Fixture checksum changed; nothing removed");
  }
  for (const [, , , destination] of files) await unlink(join(root, destination));
  await unlink(join(root, ".hypnofy-test-fixtures"));
  for (const directory of ["speech/voices", "speech", "text", ""])
    await rmdir(join(root, directory));
  console.log("Removed the verified, owned integration fixtures");
  process.exit(0);
}

await mkdir(root, { recursive: true });
await writeFile(join(root, ".hypnofy-test-fixtures"), marker);
await Promise.all(
  files.map(async ([repository, revision, file, destination, expected]) => {
    const target = resolve(root, destination);
    await mkdir(dirname(target), { recursive: true });
    const response = await fetch(
      `https://huggingface.co/${repository}/resolve/${revision}/${file}`,
    );
    if (!response.ok || !response.body) throw Error(`Fixture download failed: ${file}`);
    const hash = createHash("sha256");
    let bytes = 0;
    try {
      await pipeline(
        Readable.fromWeb(response.body),
        new Transform({
          transform(chunk, _, done) {
            bytes += chunk.length;
            hash.update(chunk);
            done(null, chunk);
          },
        }),
        createWriteStream(`${target}.partial`),
      );
      const actual = hash.digest("hex");
      if (expected && actual !== expected) throw Error(`Checksum mismatch: ${file}`);
      await rename(`${target}.partial`, target);
      console.log(`Verified ${destination}: ${bytes} bytes`);
    } catch (error) {
      await rm(`${target}.partial`, { force: true });
      throw error;
    }
  }),
);
