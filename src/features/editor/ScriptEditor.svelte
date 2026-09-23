<script lang="ts">
  import { onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import { StarterKit } from "@tiptap/starter-kit";
  import { BubbleMenu } from "@tiptap/extension-bubble-menu";
  import { PauseNode } from "./PauseNode";
  import type { Script, ScriptDocument } from "../../bridge/types";

  export let script: Script;
  export let onChange: (document: ScriptDocument) => void;
  export let onAi: (selection: string, anchor: number) => void;
  let mount: HTMLDivElement;
  let bubble: HTMLDivElement;
  let editor: Editor | null = null;
  let activeId = "";
  let slashOpen = false;
  let selectionFrom = 0;
  let selectionTo = 0;

  function emitChange(instance: Editor) {
    onChange({ version: 1, ...instance.getJSON() } as ScriptDocument);
  }
  function insertPause(seconds = 5) {
    editor?.chain().focus().insertContent({ type: "pause", attrs: { seconds } }).run();
    slashOpen = false;
  }
  function askAi() {
    if (!editor) return;
    const { from, to } = editor.state.selection;
    selectionFrom = from; selectionTo = to;
    onAi(editor.state.doc.textBetween(from, to, "\n"), from);
    slashOpen = false;
  }
  export function insertDraft(text: string, replace: boolean) {
    if (!editor) return;
    const position = replace ? { from: selectionFrom, to: selectionTo } : selectionTo || editor.state.doc.content.size;
    editor.chain().focus().insertContentAt(position, text.split(/\n\n+/).map(paragraph => ({ type: "paragraph", content: [{ type: "text", text: paragraph }] }))).run();
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === "/" && editor?.state.selection.empty) slashOpen = true;
    if (event.key === "Escape") slashOpen = false;
  }
  onMount(() => {
    editor = new Editor({
      element: mount,
      extensions: [
        StarterKit.configure({ heading: { levels: [2, 3] } }),
        PauseNode,
        BubbleMenu.configure({ element: bubble }),
      ],
      content: script.document,
      editorProps: { attributes: { class: "document-body", "aria-label": "Script body", spellcheck: "true" } },
      onUpdate: ({ editor: value }) => emitChange(value),
    });
    activeId = script.id;
    mount.addEventListener("keydown", keydown);
    return () => { mount.removeEventListener("keydown", keydown); editor?.destroy(); editor = null; };
  });
  $: if (editor && script.id !== activeId) {
    activeId = script.id;
    editor.commands.setContent(script.document, { emitUpdate: false });
    slashOpen = false;
  }
</script>

<div class="editor-tools"><button type="button" on:click={() => insertPause()}>＋ Pause</button><button type="button" on:click={askAi}>✦ Ask AI <kbd>Ctrl J</kbd></button><span>Type / for blocks</span></div>
<div bind:this={mount} class="editor-mount"></div>
<div bind:this={bubble} class="bubble-menu" aria-label="Selection actions">
  <button type="button" on:click={() => editor?.chain().focus().toggleBold().run()} aria-label="Bold">B</button>
  <button type="button" on:click={() => editor?.chain().focus().toggleItalic().run()} aria-label="Italic"><i>I</i></button>
  <button type="button" on:click={askAi}>✦ Ask AI</button>
</div>
{#if slashOpen}<div class="slash-menu" role="menu"><button on:click={() => { editor?.chain().focus().setParagraph().run(); slashOpen = false; }}>Text</button><button on:click={() => { editor?.chain().focus().toggleHeading({ level: 2 }).run(); slashOpen = false; }}>Heading</button><button on:click={() => { editor?.chain().focus().toggleBulletList().run(); slashOpen = false; }}>List</button><button on:click={() => insertPause()}>Pause</button><button on:click={askAi}>Ask AI</button></div>{/if}

<style>
  .editor-tools{display:flex;align-items:center;gap:8px;color:var(--muted);font-size:12px;margin:16px 0 24px}.editor-tools button,.bubble-menu button,.slash-menu button{border:1px solid var(--border);background:var(--surface);color:var(--text);border-radius:8px;padding:6px 9px}.editor-tools span{margin-left:auto}.editor-tools kbd{font-size:10px;color:var(--muted)}.editor-mount{min-height:55vh}.editor-mount :global(.document-body){outline:0;min-height:55vh;font-size:16px;line-height:1.8;white-space:pre-wrap}.editor-mount :global(.document-body p){margin:0 0 1.1em}.editor-mount :global(.document-body h2){font-size:24px;margin:1.5em 0 .4em}.editor-mount :global(.document-body h3){font-size:19px;margin:1.3em 0 .4em}.editor-mount :global(.document-body ul),.editor-mount :global(.document-body ol){padding-left:28px}.editor-mount :global(.pause-block){display:inline-flex;padding:6px 10px;margin:8px 0;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--accent);font-size:13px}.bubble-menu{display:flex;gap:3px;padding:5px;border:1px solid var(--border);border-radius:9px;background:var(--sidebar)}.slash-menu{position:relative;z-index:5;display:flex;gap:4px;padding:7px;width:max-content;border:1px solid var(--border);border-radius:9px;background:var(--sidebar)}
</style>
