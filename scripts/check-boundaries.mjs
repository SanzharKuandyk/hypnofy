import ts from "typescript";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { resolve, dirname, relative, sep } from "node:path";

const root = resolve("src");
const graph = new Map();

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? files(resolve(directory, entry.name))
      : /\.(ts|svelte)$/.test(entry.name)
        ? [resolve(directory, entry.name)]
        : [],
  );
}

for (const file of files(root)) {
  if (file.includes(`${sep}generated${sep}`)) continue;
  let source = readFileSync(file, "utf8");
  if (file.endsWith(".svelte"))
    source = source.match(/<script[^>]*>([\s\S]*?)<\/script>/)?.[1] ?? "";
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const imports = ast.statements
    .filter((node) => ts.isImportDeclaration(node) || ts.isExportDeclaration(node))
    .map((node) => node.moduleSpecifier)
    .filter((node) => node && ts.isStringLiteral(node))
    .map((node) => node.text)
    .filter((value) => value.startsWith("."));
  const edges = [];
  for (const specifier of imports) {
    const base = resolve(dirname(file), specifier);
    const target = [base, `${base}.ts`, `${base}.svelte`, `${base}/index.ts`].find(
      (candidate) => existsSync(candidate) && !readdirSafe(candidate),
    );
    if (!target) continue;
    const from = relative(root, file).split(sep),
      to = relative(root, target).split(sep);
    if (from[0] === "shared" && !["shared", "domain"].includes(to[0]))
      throw Error(`Shared infrastructure imports application code: ${file}`);
    if (from[0] === "shared" && ["ui", "layout"].includes(from[1]) && to[0] === "domain")
      throw Error(`UI primitive imports business contracts: ${file}`);
    if (from[0] === "domain" && target.endsWith(".svelte"))
      throw Error(`Domain imports a component: ${file}`);
    if (
      from[0] === "modules" &&
      to[0] === "modules" &&
      from[1] !== to[1] &&
      to.at(-1) !== "index.ts"
    )
      throw Error(`Module imports another module's private file: ${file} -> ${target}`);
    edges.push(target);
  }
  graph.set(file, edges);
}

function readdirSafe(path) {
  try {
    readdirSync(path);
    return true;
  } catch {
    return false;
  }
}

const visited = new Set(),
  active = new Set();
function visit(file) {
  if (active.has(file))
    throw Error(`Import cycle: ${[...active, file].map((p) => relative(root, p)).join(" -> ")}`);
  if (visited.has(file)) return;
  active.add(file);
  for (const edge of graph.get(file) ?? []) visit(edge);
  active.delete(file);
  visited.add(file);
}
for (const file of graph.keys()) visit(file);
console.log(`Module boundaries and cycles verified across ${graph.size} source files`);
