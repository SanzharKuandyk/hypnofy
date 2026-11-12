<script lang="ts">
  import { onDestroy } from "svelte";
  import { DropdownMenu } from "bits-ui";
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
    MoreHorizontal,
  } from "@lucide/svelte";
  import { Button, Dialog, Field, Select, Input, Checkbox } from "../shared/ui";
  import { api } from "../shared/rpc/client";
  import { chooseExportFolder } from "../shared/platform/dialogs";
  import type { Workspace } from "../app/workflows/workspace.svelte";

  let { workspace: w, oncollection }: { workspace: Workspace; oncollection: () => void } = $props();
  let selected = $state<string[]>([]);
  const visibleSelection = $derived(
    w.scripts.filter((item) => selected.includes(item.id)).map((item) => item.id),
  );
  let actionIds = $state<string[]>([]);
  let renameScriptOpen = $state(false);
  let renameScriptId = $state("");
  let scriptName = $state("");
  let renameError = $state("");
  let renamingScript = $state(false);
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

  async function bulk(
    operation: string,
    collectionId: string | null = null,
    ids = visibleSelection,
  ): Promise<void> {
    await w.bulk(ids, operation, collectionId);
    selected = [];
    confirm = false;
    move = false;
  }

  async function saveScriptName(): Promise<void> {
    if (!scriptName.trim() || renamingScript) return;
    renamingScript = true;
    renameError = "";
    try {
      await w.renameScript(renameScriptId, scriptName);
      renameScriptOpen = false;
    } catch (value) {
      renameError = value instanceof Error ? value.message : "Could not rename the script.";
    } finally {
      renamingScript = false;
    }
  }

  function search(): void {
    selected = [];
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
      <Search size={16} /><Input
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
            actionIds = scripts.map((s) => s.id);
            confirm = true;
          })}>Empty trash</Button
      >{:else}<Button onclick={() => w.action(() => w.importScript())}
        ><Upload size={15} />Import</Button
      >{/if}
  </div>
  {#if visibleSelection.length}<div
      class="script-selection"
      role="group"
      aria-label="Selected script actions"
    >
      <strong>{visibleSelection.length} selected</strong>{#if trash}<Button
          onclick={() => w.action(() => bulk("restore"))}><RotateCcw size={15} />Restore</Button
        ><Button
          variant="danger"
          onclick={() => {
            actionIds = visibleSelection;
            confirm = true;
          }}>Delete permanently</Button
        >{:else}<Button
          onclick={() => {
            actionIds = visibleSelection;
            move = true;
          }}>Move</Button
        ><Button onclick={() => w.action(() => bulk("favorite"))}>Favorite</Button><Button
          onclick={() => {
            actionIds = visibleSelection;
            exportOpen = true;
          }}>Export</Button
        ><Button variant="danger" onclick={() => w.action(() => bulk("trash"))}
          >Move to Trash</Button
        >{/if}<Button variant="ghost" onclick={() => (selected = [])}>Clear</Button>
    </div>{/if}
  {#if w.scripts.length}
    <table class="script-table">
      <thead
        ><tr
          ><th
            ><Checkbox
              label="Select all scripts"
              checked={w.scripts.every((script) => selected.includes(script.id))}
              indeterminate={w.scripts.some((script) => selected.includes(script.id)) &&
                !w.scripts.every((script) => selected.includes(script.id))}
              onchange={(value) => (selected = value ? w.scripts.map((s) => s.id) : [])}
            /></th
          ><th>Name</th><th>Collection</th><th>Edited</th><th>Favorite</th><th
            ><span class="muted">Actions</span></th
          ></tr
        ></thead
      ><tbody>
        {#each w.scripts as script (script.id)}<tr class:selected={selected.includes(script.id)}
            ><td
              ><Checkbox
                label={`Select ${script.title}`}
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
                      actionIds = [script.id];
                      confirm = true;
                    }}><Trash2 size={16} /></Button
                  >{:else}
                  <DropdownMenu.Root>
                    <DropdownMenu.Trigger
                      class="button ghost icon"
                      aria-label={`Actions for ${script.title}`}
                    >
                      <MoreHorizontal size={18} />
                    </DropdownMenu.Trigger>
                    <DropdownMenu.Portal>
                      <DropdownMenu.Content
                        class="script-actions-menu"
                        align="end"
                        sideOffset={6}
                        collisionPadding={12}
                      >
                        <DropdownMenu.Item
                          onSelect={() => {
                            renameScriptId = script.id;
                            scriptName = script.title;
                            renameError = "";
                            renameScriptOpen = true;
                          }}><Pencil size={16} />Rename</DropdownMenu.Item
                        >
                        <DropdownMenu.Item
                          onSelect={() => {
                            target = script.collectionId;
                            actionIds = [script.id];
                            move = true;
                          }}><Folder size={16} />Move to collection</DropdownMenu.Item
                        >
                        <DropdownMenu.Item
                          onSelect={() =>
                            w.action(() =>
                              w.bulk([script.id], script.favorite ? "unfavorite" : "favorite"),
                            )}
                        >
                          <Star size={16} />{script.favorite
                            ? "Remove favorite"
                            : "Add to favorites"}
                        </DropdownMenu.Item>
                        <DropdownMenu.Item
                          onSelect={() =>
                            w.action(async () => {
                              const copy = await api.scripts.duplicate(script.id);
                              await w.navigate({ kind: "script", id: copy.id });
                            })}><Copy size={16} />Duplicate</DropdownMenu.Item
                        >
                        <DropdownMenu.Item
                          onSelect={() => w.action(() => w.exportScript(script.id, script.title))}
                        >
                          <Download size={16} />Export</DropdownMenu.Item
                        >
                        <DropdownMenu.Separator />
                        <DropdownMenu.Item
                          class="danger-item"
                          onSelect={() => w.action(() => w.bulk([script.id], "trash"))}
                        >
                          <Trash2 size={16} />Move to Trash
                        </DropdownMenu.Item>
                      </DropdownMenu.Content>
                    </DropdownMenu.Portal>
                  </DropdownMenu.Root>
                {/if}
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
      onclick={() => w.action(() => bulk("delete", null, actionIds))}>Delete permanently</Button
    >
  </div></Dialog
>
<Dialog bind:open={move} title="Move scripts"
  ><Field label="Collection"
    ><Select
      label="Destination collection"
      bind:value={target}
      options={[...w.collections.map((item) => ({ value: item.id, label: `${item.name}` }))]}
    /></Field
  >
  <div class="actions">
    <Button onclick={() => (move = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() => w.action(() => bulk("move", target, actionIds))}>Move scripts</Button
    >
  </div></Dialog
>
<Dialog bind:open={renameScriptOpen} title="Rename script" required={renamingScript}>
  <form
    class="rename-script-form"
    onsubmit={(event) => {
      event.preventDefault();
      void saveScriptName();
    }}
  >
    <Field label="Name"
      ><Input aria-label="Script name" bind:value={scriptName} disabled={renamingScript} /></Field
    >
    {#if renameError}<p class="inline-error" role="alert">{renameError}</p>{/if}
    <div class="actions">
      <Button disabled={renamingScript} onclick={() => (renameScriptOpen = false)}>Cancel</Button>
      <Button variant="primary" type="submit" loading={renamingScript} disabled={!scriptName.trim()}
        >Save name</Button
      >
    </div>
  </form>
</Dialog>

<Dialog bind:open={rename} title="Rename collection"
  ><Field label="Name"><Input aria-label="Collection name" bind:value={collectionName} /></Field>
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
    ><Select
      label="Bulk export format"
      bind:value={exportFormat}
      options={[
        { value: "json", label: `Hypnofy JSON · lossless` },
        { value: "md", label: `Markdown` },
        { value: "txt", label: `Plain text` },
      ]}
    /></Field
  >
  <div class="actions">
    <Button onclick={() => (exportOpen = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() =>
        w.action(async () => {
          await w.editor.flush();
          const folder = await chooseExportFolder();
          if (folder) {
            await api.scripts.exportMany(actionIds, folder, exportFormat);
            exportOpen = false;
            selected = [];
            w.notify("Script export started");
          }
        })}>Choose folder & export</Button
    >
  </div></Dialog
>
