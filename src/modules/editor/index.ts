import type * as EditorModule from "./ui/ScriptEditor.svelte";

export { EditorSession } from "./application/session.svelte";
export type { EditorSelection } from "./model/selection";

export function loadScriptEditor(): Promise<typeof EditorModule> {
  return import("./ui/ScriptEditor.svelte");
}
