<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    FileText,
    Folder,
    ChevronRight,
    Plus,
    Search,
    Star,
    Copy,
    Trash2,
    RotateCcw,
    Upload,
    Download,
    Pencil,
  } from "@lucide/svelte";
  import { Button, Dialog, Field } from "../shared/ui";
  import { api } from "../shared/rpc/client";
  import { chooseExportFolder } from "../shared/platform/dialogs";
  import type { Workspace } from "../app/workflows/workspace.svelte";

  let { workspace: w, oncollection }: { workspace: Workspace; oncollection: () => void } = $props();
  let selected = $state<string[]>([]);
  let confirm = $state(false);
  let move = $state(false);
  let target = $state("inbox");
  let rename = $state(false);
  let collectionName = $state("");
  let deleteCollection = $state(false);
  let exportOpen = $state(false);
  let exportFormat = $state<"txt" | "md" | "json">("json");
  let localSearch = $derived(w.query);
  let searchTimer: ReturnType<typeof setTimeout>;
  onDestroy(() => clearTimeout(searchTimer));
  const route = $derived(w.route.kind === "library" ? w.route : null);
  const trash = $derived(route?.view === "trash");
  const collection = $derived(w.collections.find((c) => c.id === route?.collectionId));
  const title = $derived(
    trash
      ? "Trash"
      : route?.view === "favorites"
        ? "Favorites"
        : (collection?.name ?? "All scripts"),
  );

  function toggle(id: string): void {
    selected = selected.includes(id) ? selected.filter((value) => value !== id) : [...selected, id];
  }

  async function bulk(operation: string, collectionId: string | null = null): Promise<void> {
    await w.bulk(selected, operation, collectionId);
    selected = [];
    confirm = false;
    move = false;
  }

  function search(): void {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      w.query = localSearch;
      void w.action(() => w.refreshLibrary());
    }, 250);
  }
</script>

<div class="page">
  <header class="page-heading">
    <div>
      <h1>{title}</h1>
      <p>
        {trash
          ? "Deleted scripts stay here until you choose to remove them."
          : "A quiet place for your words, ready when you are."}
      </p>
    </div>
    <Button variant="primary" onclick={() => w.action(() => w.createScript())}
      ><Plus size={16} />New script</Button
    >
  </header>
  {#if !trash && !collection && route?.view === "all"}
    <div class="row spread">
      <h3>Your collections</h3>
      <Button variant="ghost" onclick={oncollection}><Plus size={14} />New collection</Button>
    </div>
    <div class="collections">
      {#each w.collections.slice(0, 3) as item (item.id)}<button
          class="collection-card"
          onclick={() =>
            w.action(() => w.navigate({ kind: "library", view: "all", collectionId: item.id }))}
          ><Folder size={22} /><span
            ><strong>{item.name}</strong><small>{item.scriptCount} scripts</small></span
          ><ChevronRight size={16} /></button
        >{/each}
    </div>
  {/if}
  <div class="toolbar">
    <div class="search-field">
      <Search size={16} /><input
        aria-label="Search scripts"
        placeholder="Search scripts…"
        bind:value={localSearch}
        oninput={search}
      />
    </div>
    {#if collection && collection.id !== "inbox"}<Button
        variant="ghost"
        aria-label="Rename collection"
        onclick={() => {
          collectionName = collection.name;
          rename = true;
        }}><Pencil size={15} />Rename</Button
      ><Button
        variant="ghost"
        aria-label="Delete collection"
        onclick={() => (deleteCollection = true)}><Trash2 size={15} /></Button
      >{/if}
    <span class="push"></span>
    {#if trash}<Button
        variant="danger"
        disabled={!w.scripts.length}
        onclick={() =>
          w.action(async () => {
            const scripts = await api.library.scripts({
              collectionId: null,
              query: "",
              favorite: false,
              trash: true,
            });
            selected = scripts.map((s) => s.id);
            confirm = true;
          })}>Empty trash</Button
      >{:else}<Button onclick={() => w.action(() => w.importScript())}
        ><Upload size={15} />Import</Button
      >{/if}
  </div>
  {#if selected.length}<div class="toolbar card">
      <strong>{selected.length} selected</strong>{#if trash}<Button
          onclick={() => w.action(() => bulk("restore"))}><RotateCcw size={15} />Restore</Button
        ><Button variant="danger" onclick={() => (confirm = true)}>Delete permanently</Button
        >{:else}<Button onclick={() => (move = true)}>Move</Button><Button
          onclick={() => w.action(() => bulk("favorite"))}>Favorite</Button
        ><Button onclick={() => (exportOpen = true)}>Export</Button><Button
          variant="danger"
          onclick={() => w.action(() => bulk("trash"))}>Move to Trash</Button
        >{/if}<Button variant="ghost" onclick={() => (selected = [])}>Clear</Button>
    </div>{/if}
  {#if w.scripts.length}
    <table class="script-table">
      <thead
        ><tr
          ><th
            ><input
              type="checkbox"
              aria-label="Select all scripts"
              checked={selected.length === w.scripts.length}
              onchange={(event) =>
                (selected = event.currentTarget.checked ? w.scripts.map((s) => s.id) : [])}
            /></th
          ><th>Name</th><th>Collection</th><th>Edited</th><th>Favorite</th><th
            ><span class="muted">Actions</span></th
          ></tr
        ></thead
      ><tbody>
        {#each w.scripts as script (script.id)}<tr class:selected={selected.includes(script.id)}
            ><td
              ><input
                type="checkbox"
                aria-label={`Select ${script.title}`}
                checked={selected.includes(script.id)}
                onchange={() => toggle(script.id)}
              /></td
            ><td
              ><button
                class="script-title"
                onclick={() => w.action(() => w.navigate({ kind: "script", id: script.id }))}
                ><FileText size={18} /><span>{script.title}</span></button
              ></td
            ><td class="muted">{script.collectionName}</td><td class="muted"
              >{new Date(script.updatedAt).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
              })}</td
            ><td
              ><Button
                variant="ghost"
                class="icon"
                aria-label={script.favorite ? "Remove favorite" : "Favorite script"}
                onclick={() =>
                  w.action(() => w.bulk([script.id], script.favorite ? "unfavorite" : "favorite"))}
                ><Star size={16} fill={script.favorite ? "currentColor" : "none"} /></Button
              ></td
            ><td
              ><div class="row-actions">
                {#if trash}<Button
                    variant="ghost"
                    class="icon"
                    aria-label="Restore script"
                    onclick={() => w.action(() => w.bulk([script.id], "restore"))}
                    ><RotateCcw size={16} /></Button
                  ><Button
                    variant="ghost"
                    class="icon"
                    aria-label="Permanently delete script"
                    onclick={() => {
                      selected = [script.id];
                      confirm = true;
                    }}><Trash2 size={16} /></Button
                  >{:else}<Button
                    variant="ghost"
                    class="icon optional"
                    aria-label="Duplicate script"
                    onclick={() =>
                      w.action(async () => {
                        const copy = await api.scripts.duplicate(script.id);
                        await w.navigate({ kind: "script", id: copy.id });
                      })}><Copy size={16} /></Button
                  ><Button
                    variant="ghost"
                    class="icon optional"
                    aria-label="Export script"
                    onclick={() => w.action(() => w.exportScript(script.id, script.title))}
                    ><Download size={16} /></Button
                  ><Button
                    variant="ghost"
                    class="icon"
                    aria-label="Move script to Trash"
                    onclick={() => w.action(() => w.bulk([script.id], "trash"))}
                    ><Trash2 size={16} /></Button
                  >{/if}
              </div></td
            ></tr
          >{/each}
      </tbody>
    </table>
    <div class="table-caption">
      <span>{w.scripts.length} scripts · saved on this device</span><span
        >Write at your own pace.</span
      >
    </div>
  {:else}<div class="empty-state">
      <FileText size={32} />
      <h2>
        {localSearch
          ? "No matching scripts"
          : trash
            ? "Your trash is empty"
            : "Make room for a little calm"}
      </h2>
      <p>
        {localSearch
          ? "Try a different title or phrase."
          : "Start with your own words, or shape a draft with AI."}
      </p>
      {#if !trash}<Button variant="primary" onclick={() => w.action(() => w.createScript())}
          >Create your first script</Button
        >{/if}
    </div>{/if}
  {#if !trash}<div class="hint">
      <FileText size={18} /><span
        >Small details make a session yours. Add pauses, adjust the pace, and choose a voice.</span
      ><Button variant="ghost" onclick={() => (w.help = true)}>Learn more</Button>
    </div>{/if}
</div>

<Dialog
  bind:open={confirm}
  title="Delete permanently?"
  description="This removes the selected scripts, their revisions and saved audio. You cannot undo this."
  ><div class="actions">
    <Button onclick={() => (confirm = false)}>Cancel</Button><Button
      variant="danger"
      onclick={() => w.action(() => bulk("delete"))}>Delete permanently</Button
    >
  </div></Dialog
>
<Dialog bind:open={move} title="Move scripts"
  ><Field label="Collection"
    ><select aria-label="Destination collection" bind:value={target}
      >{#each w.collections as item (item.id)}<option value={item.id}>{item.name}</option
        >{/each}</select
    ></Field
  >
  <div class="actions">
    <Button onclick={() => (move = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() => w.action(() => bulk("move", target))}>Move scripts</Button
    >
  </div></Dialog
>
<Dialog bind:open={rename} title="Rename collection"
  ><Field label="Name"><input aria-label="Collection name" bind:value={collectionName} /></Field>
  <div class="actions">
    <Button onclick={() => (rename = false)}>Cancel</Button><Button
      variant="primary"
      disabled={!collectionName.trim()}
      onclick={() =>
        w.action(async () => {
          if (collection) await api.library.renameCollection(collection.id, collectionName);
          rename = false;
          await w.refreshLibrary();
        })}>Save name</Button
    >
  </div></Dialog
>
<Dialog
  bind:open={deleteCollection}
  title="Delete collection?"
  description="Its scripts will move to My scripts. Your writing will be kept."
  ><div class="actions">
    <Button onclick={() => (deleteCollection = false)}>Cancel</Button><Button
      variant="danger"
      onclick={() =>
        w.action(async () => {
          if (collection) await api.library.deleteCollection(collection.id);
          deleteCollection = false;
          await w.navigate({ kind: "library", view: "all", collectionId: null });
        })}>Delete collection</Button
    >
  </div></Dialog
>

<Dialog
  bind:open={exportOpen}
  title="Export selected scripts"
  description="A new folder is published only after every selected script exports successfully. Existing files are kept."
  ><Field label="Format"
    ><select aria-label="Bulk export format" bind:value={exportFormat}
      ><option value="json">Hypnofy JSON · lossless</option><option value="md">Markdown</option
      ><option value="txt">Plain text</option></select
    ></Field
  >
  <div class="actions">
    <Button onclick={() => (exportOpen = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() =>
        w.action(async () => {
          await w.editor.flush();
          const folder = await chooseExportFolder();
          if (folder) {
            await api.scripts.exportMany(selected, folder, exportFormat);
            exportOpen = false;
            selected = [];
            w.notify("Script export started");
          }
        })}>Choose folder & export</Button
    >
  </div></Dialog
>
