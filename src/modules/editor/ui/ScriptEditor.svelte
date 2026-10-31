<script lang="ts">
  import { onMount } from "svelte";
  import { Editor, Extension, Mark, Node } from "@tiptap/core";
  import { StarterKit } from "@tiptap/starter-kit";
  import { ListItem } from "@tiptap/extension-list";
  import { Plugin } from "@tiptap/pm/state";
  import type { ResolvedPos } from "@tiptap/pm/model";
  import type { TextAnchor, Suggestion } from "../../../shared/rpc/generated/bindings";
  import { textHash } from "../model/selection";
  import type { EditorSelection } from "../model/selection";
  import {
    Bold,
    Italic,
    Strikethrough,
    List,
    ListOrdered,
    Plus,
    Sparkles,
    Link2,
    Undo2,
    Redo2,
  } from "@lucide/svelte";
  import { Button, Select, NumberField } from "../../../shared/ui";
  import { decodeDocument } from "../../../domain/document";
  import type { ScriptDocument, DeliveryOverride } from "../../../domain/document";

  let {
    document,
    onchange,
    onask,
    onselection,
  }: {
    document: ScriptDocument;
    onchange: (value: ScriptDocument) => void;
    onask: (selection: EditorSelection) => void;
    onselection?: (hasSelection: boolean) => void;
  } = $props();

  let mount: HTMLDivElement;
  let editor: Editor | null = null;
  let selectedFrom = 0;
  let selectedTo = 0;
  let slash = $state(false);
  let pauseOpen = $state(false);
  let seconds = $state(5);
  let paragraphStyle = $state("text");
  let selectedPause: number | null = null;

  const blocks = ["paragraph", "heading", "bulletList", "orderedList", "listItem", "pause"];

  const StableBlocks = Extension.create({
    name: "stableBlocks",
    addGlobalAttributes() {
      return [
        {
          types: blocks,
          attributes: {
            id: {
              default: null,
              renderHTML: (attrs) => (attrs.id ? { "data-block-id": attrs.id } : {}),
            },
            delivery: { default: null, rendered: false },
            pauseAfterMs: { default: null, rendered: false },
            flowGroupId: {
              default: null,
              renderHTML: (attrs) =>
                attrs.flowGroupId ? { "data-flow-group": attrs.flowGroupId } : {},
            },
          },
        },
      ];
    },
    addProseMirrorPlugins() {
      return [
        new Plugin({
          appendTransaction: (_transactions, _old, state) => {
            const transaction = state.tr;
            // eslint-disable-next-line svelte/prefer-svelte-reactivity -- Ephemeral validation within a ProseMirror transaction.
            const ids = new Set<string>();
            let changed = false;
            state.doc.descendants((node, position) => {
              if (blocks.includes(node.type.name)) {
                let id: unknown = node.attrs.id;
                if (typeof id !== "string" || !id || ids.has(id)) {
                  id = crypto.randomUUID();
                  transaction.setNodeMarkup(position, undefined, { ...node.attrs, id });
                  changed = true;
                }
                if (typeof id === "string") ids.add(id);
              }
            });
            return changed ? transaction : null;
          },
        }),
      ];
    },
  });

  const Pause = Node.create({
    name: "pause",
    group: "block",
    atom: true,
    selectable: true,
    addAttributes: () => ({ durationMs: { default: 5000 } }),
    parseHTML: () => [{ tag: "div[data-pause]" }],
    renderHTML: ({ node }) => [
      "div",
      { "data-pause": "", contenteditable: "false" },
      `◷  Pause · ${Number(node.attrs.durationMs) / 1000} seconds`,
    ],
  });

  const InlinePause = Node.create({
    name: "inlinePause",
    group: "inline",
    inline: true,
    atom: true,
    addAttributes: () => ({ durationMs: { default: 5000 } }),
    parseHTML: () => [{ tag: "span[data-pause]" }],
    renderHTML: ({ node }) => [
      "span",
      { "data-pause": "", contenteditable: "false" },
      `◷ ${Number(node.attrs.durationMs) / 1000}s`,
    ],
  });

  const Delivery = Mark.create({
    name: "delivery",
    addAttributes: () => ({ voiceId: { default: null }, speed: { default: null } }),
    parseHTML: () => [{ tag: "span[data-delivery]" }],
    renderHTML: ({ HTMLAttributes }) => ["span", { ...HTMLAttributes, "data-delivery": "" }, 0],
  });

  function anchor(position: ResolvedPos): TextAnchor | null {
    for (let depth = position.depth; depth > 0; depth--) {
      const node = position.node(depth);
      if (["paragraph", "heading"].includes(node.type.name) && typeof node.attrs.id === "string")
        return { blockId: node.attrs.id, offsetUtf16: position.pos - position.start(depth) };
    }
    let nearest: TextAnchor | null = null;
    editor?.state.doc.descendants((node, pos) => {
      if (nearest && position.pos < pos) return false;
      if (
        ["paragraph", "heading", "pause"].includes(node.type.name) &&
        typeof node.attrs.id === "string"
      ) {
        const start = pos + (node.type.name === "pause" ? 0 : 1);
        const size = node.type.name === "pause" ? 1 : node.content.size;
        nearest = {
          blockId: node.attrs.id,
          offsetUtf16: Math.min(size, Math.max(0, position.pos - start)),
        };
      }
      return true;
    });
    return nearest;
  }

  function position(anchor: TextAnchor): number {
    if (!editor) throw new Error("Editor unavailable");
    let found: number | undefined;
    editor.state.doc.descendants((node, pos) => {
      if (
        node.attrs.id === anchor.blockId &&
        anchor.offsetUtf16 <= (node.type.name === "pause" ? 1 : node.content.size)
      )
        found = pos + (node.type.name === "pause" ? 0 : 1) + anchor.offsetUtf16;
    });
    if (found === undefined)
      throw new Error("The insertion passage changed. Review a new suggestion.");
    return found;
  }

  export function applySuggestion(text: string, replace: boolean, suggestion: Suggestion): void {
    if (!editor) throw new Error("The editor is unavailable");
    const content = text
      .trim()
      .split(/\n\n+/)
      .filter((text) => text.length > 0)
      .map((text) => ({
        type: "paragraph",
        attrs: { id: crypto.randomUUID() },
        content: [{ type: "text", text }],
      }));
    const range = suggestion.range;
    const from = range ? position(range.from) : selectedFrom;
    const to = range
      ? position(range.to)
      : suggestion.anchor
        ? position(suggestion.anchor)
        : selectedTo || editor.state.doc.content.size;
    editor
      .chain()
      .focus()
      .insertContentAt(
        replace
          ? suggestion.operation === "new"
            ? { from: 0, to: editor.state.doc.content.size }
            : { from, to }
          : to,
        content,
      )
      .run();
  }

  export function applyDelivery(value: DeliveryOverride): void {
    if (!editor) return;
    if (!editor.state.selection.empty) editor.chain().focus().setMark("delivery", value).run();
    else {
      const from = editor.state.selection.$from;
      for (let depth = from.depth; depth > 0; depth--) {
        const node = from.node(depth);
        if (["paragraph", "heading"].includes(node.type.name)) {
          editor.view.dispatch(
            editor.state.tr.setNodeMarkup(from.before(depth), undefined, {
              ...node.attrs,
              delivery: value,
            }),
          );
          break;
        }
      }
    }
  }

  export function makeContinuous(): void {
    if (!editor) return;
    const { from, to } = editor.state.selection;
    const group = crypto.randomUUID();
    const transaction = editor.state.tr;
    editor.state.doc.nodesBetween(from, to, (node, position) => {
      if (["paragraph", "heading"].includes(node.type.name))
        transaction.setNodeMarkup(position, undefined, { ...node.attrs, flowGroupId: group });
    });
    editor.view.dispatch(transaction);
  }

  function insertPause(): void {
    if (!Number.isFinite(seconds) || seconds < 0.5 || seconds > 120) return;
    const durationMs = Math.round(Math.min(120, Math.max(0.5, seconds)) * 1000);
    if (editor && selectedPause !== null) {
      const node = editor.state.doc.nodeAt(selectedPause);
      if (node && ["pause", "inlinePause"].includes(node.type.name)) {
        editor.view.dispatch(
          editor.state.tr.setNodeMarkup(selectedPause, undefined, { ...node.attrs, durationMs }),
        );
        pauseOpen = false;
        return;
      }
    }
    editor
      ?.chain()
      .focus()
      .insertContent({
        type: editor.state.selection.$from.parent.isTextblock ? "inlinePause" : "pause",
        attrs: { durationMs },
      })
      .run();
    pauseOpen = false;
    slash = false;
  }

  async function ask(): Promise<void> {
    if (!editor) return;
    selectedFrom = editor.state.selection.from;
    selectedTo = editor.state.selection.to;
    const from = anchor(editor.state.selection.$from);
    const to = anchor(editor.state.selection.$to);
    const text = editor.state.doc.textBetween(selectedFrom, selectedTo, "\n", (node) =>
      node.type.name === "hardBreak" ? "\n" : "\ufffc",
    );
    const hash = await textHash(text);
    onask({
      text,
      anchor: to,
      range:
        from && to && selectedFrom !== selectedTo ? { from, to, selectedTextHash: hash } : null,
    });
    slash = false;
  }

  function keydown(event: KeyboardEvent): void {
    if (
      event.key === "/" &&
      editor?.state.selection.empty &&
      !editor.state.selection.$from.parent.textContent.trim()
    ) {
      event.preventDefault();
      slash = true;
    }
    if (event.key === "Escape") {
      slash = false;
      pauseOpen = false;
    }
  }

  onMount(() => {
    editor = new Editor({
      element: mount,
      content: document,
      extensions: [
        StarterKit.configure({
          heading: { levels: [2, 3] },
          listItem: false,
          blockquote: false,
          codeBlock: false,
          code: false,
          horizontalRule: false,
          link: false,
          underline: false,
        }),
        ListItem.extend({ content: "block+" }),
        StableBlocks,
        Pause,
        InlinePause,
        Delivery,
      ],
      editorProps: {
        attributes: { class: "prose", "aria-label": "Script body", spellcheck: "true" },
      },
      onUpdate: ({ editor: instance }) =>
        onchange(decodeDocument({ ...instance.getJSON(), version: 2 })),
      onSelectionUpdate: ({ editor: instance }) => {
        paragraphStyle = instance.isActive("heading", {level:2}) ? "2" : instance.isActive("heading", {level:3}) ? "3" : "text";
        const { from, to } = instance.state.selection;
        const node = instance.state.doc.nodeAt(from);
        if (
          node &&
          to === from + node.nodeSize &&
          ["pause", "inlinePause"].includes(node.type.name)
        ) {
          selectedPause = from;
          seconds = Number(node.attrs.durationMs) / 1000;
          pauseOpen = true;
        } else selectedPause = null;
        onselection?.(!instance.state.selection.empty);
      },
      onCreate: ({ editor: instance }) => instance.view.dom.addEventListener("keydown", keydown),
    });
    return () => {
      editor?.destroy();
      editor = null;
    };
  });
</script>

<div class="toolbar editor-formatting" aria-label="Text formatting">
  <Button
    variant="ghost"
    class="icon"
    title="Bold"
    aria-label="Bold"
    onclick={() => editor?.chain().focus().toggleBold().run()}><Bold size={16} /></Button
  >
  <Button
    variant="ghost"
    class="icon"
    title="Italic"
    aria-label="Italic"
    onclick={() => editor?.chain().focus().toggleItalic().run()}><Italic size={16} /></Button
  >
  <Button
    variant="ghost"
    class="icon"
    title="Strikethrough"
    aria-label="Strikethrough"
    onclick={() => editor?.chain().focus().toggleStrike().run()}><Strikethrough size={16} /></Button
  >
  <Select
    label="Paragraph style"
    onclose={() => editor?.commands.focus()}
    style="width:118px"
    onchange={(value) => {
      const level = value;
      if (level === "2" || level === "3")
        editor
          ?.chain()
          .focus()
          .setHeading({ level: level === "2" ? 2 : 3 })
          .run();
      else editor?.chain().focus().setParagraph().run();
    }}
    bind:value={paragraphStyle}
    options={[
      { value: "text", label: `Text` },
      { value: "2", label: `Heading 2` },
      { value: "3", label: `Heading 3` },
    ]}
  />
  <Button
    variant="ghost"
    class="icon"
    title="List"
    aria-label="List"
    onclick={() => editor?.chain().focus().toggleBulletList().run()}><List size={18} /></Button
  >
  <Button
    variant="ghost"
    class="icon"
    aria-label="Numbered list"
    title="Numbered list"
    onclick={() => editor?.chain().focus().toggleOrderedList().run()}
    ><ListOrdered size={18} /></Button
  >
  <Button
    variant="ghost"
    class="icon"
    aria-label="Undo"
    title="Undo"
    onclick={() => editor?.chain().focus().undo().run()}><Undo2 size={16} /></Button
  >
  <Button
    variant="ghost"
    class="icon"
    aria-label="Redo"
    title="Redo"
    onclick={() => editor?.chain().focus().redo().run()}><Redo2 size={16} /></Button
  >
  <Button variant="ghost" onclick={() => (pauseOpen = !pauseOpen)}><Plus size={15} />Pause</Button>
  <Button variant="ghost" title="Make selected passages continuous" onclick={makeContinuous}
    ><Link2 size={15} />Continuous</Button
  >
  <Button variant="ghost" onclick={ask}><Sparkles size={15} />Ask AI</Button>
</div>
{#if pauseOpen}<div class="row">
    <label for="pause-seconds">Pause in seconds</label><NumberField
      label="Pause in seconds"
      id="pause-seconds"
      min={0.5}
      max={120}
      step={0.5}
      bind:value={seconds}
      style="width:170px"
    /><Button
      disabled={!Number.isFinite(seconds) || seconds < 0.5 || seconds > 120}
      onclick={insertPause}>Insert pause</Button
    >
  </div>{/if}
{#if slash}<div class="toolbar">
    <Button
      onclick={() => {
        editor?.chain().focus().setParagraph().run();
        slash = false;
      }}>Text</Button
    ><Button
      onclick={() => {
        editor?.chain().focus().toggleHeading({ level: 2 }).run();
        slash = false;
      }}>Heading</Button
    ><Button onclick={() => (pauseOpen = true)}>Pause</Button><Button onclick={ask}>Ask AI</Button>
  </div>{/if}
<div bind:this={mount} role="group" aria-label="Script editor"></div>
