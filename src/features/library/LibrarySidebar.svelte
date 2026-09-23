<script lang="ts">
  import type { Collection, ScriptSummary } from "../../bridge/types";
  export let collections: Collection[];
  export let scripts: ScriptSummary[];
  export let currentId: string | null;
  export let view: string;
  export let search: string;
  export let onView: (value: string) => void;
  export let onSearch: (value: string) => void;
  export let onScript: (id: string) => void;
  export let onNew: () => void;
  export let onCollection: () => void;
  export let onImport: () => void;
</script>
<aside class="library" aria-label="Script library">
  <div class="brand">◐ <strong>Hypnofy</strong></div>
  <button class="new" on:click={onNew}>＋ &nbsp; New script</button>
  <label class="search"><span>⌕</span><input value={search} on:input={event => onSearch(event.currentTarget.value)} placeholder="Search scripts" aria-label="Search scripts" /></label>
  <nav aria-label="Library views">
    <button class:active={view === "all"} on:click={() => onView("all")}>▤ &nbsp; All scripts</button>
    <button class:active={view === "favorites"} on:click={() => onView("favorites")}>☆ &nbsp; Favorites</button>
    <div class="nav-heading">Collections <button title="New collection" on:click={onCollection}>＋</button></div>
    {#each collections as collection}<button class:active={view === `collection:${collection.id}`} on:click={() => onView(`collection:${collection.id}`)}><span class="collection-dot">●</span> {collection.name} <small>{collection.scriptCount}</small></button>{/each}
    <div class="nav-heading">Library</div><button class:active={view === "trash"} on:click={() => onView("trash")}>♲ &nbsp; Trash</button>
  </nav>
  <div class="script-heading">{view === "trash" ? "Trash" : view === "favorites" ? "Favorites" : "Recent scripts"}</div>
  <div class="script-items">{#each scripts as script}<button class:active={script.id === currentId} on:click={() => onScript(script.id)}><strong>{script.title}</strong><small>{script.collectionName}</small></button>{:else}<p>No scripts here yet.</p>{/each}</div>
  <div class="bottom"><button on:click={onImport}>Import text</button></div>
</aside>
<style>
  .library{display:flex;flex-direction:column;min-width:0;padding:18px 12px 12px;background:var(--sidebar);border-right:1px solid var(--border);overflow:auto}.brand{display:flex;gap:9px;align-items:center;padding:2px 9px 24px;font-size:16px}.brand:first-letter{color:var(--accent)}.new{width:100%;height:37px;text-align:left;padding:0 11px;border:1px solid var(--border);border-radius:var(--radius);background:var(--surface);color:var(--text)}.search{display:flex;align-items:center;gap:8px;margin:15px 0 10px;padding:7px 10px;border:1px solid var(--border);border-radius:var(--radius);color:var(--muted)}.search input{min-width:0;width:100%;border:0;background:none;color:var(--text);outline:0}.library nav{display:grid;gap:3px}.library nav>button,.bottom button{width:100%;border:0;background:transparent;color:var(--muted);padding:8px 10px;text-align:left;border-radius:8px}.library nav>button:hover,.library nav>button.active,.bottom button:hover{background:var(--surface);color:var(--text)}.library nav small{float:right;color:var(--muted)}.collection-dot{color:var(--accent);font-size:9px}.nav-heading{display:flex;justify-content:space-between;margin:20px 10px 5px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted);font-size:10px}.nav-heading button{border:0;background:none;color:var(--muted)}.script-heading{margin:28px 10px 8px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted);font-size:10px}.script-items{display:grid;gap:3px;overflow:auto}.script-items button{display:grid;gap:3px;min-width:0;width:100%;padding:9px 10px;text-align:left;border:0;border-radius:8px;background:none;color:var(--text)}.script-items button.active,.script-items button:hover{background:var(--surface)}.script-items strong{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:13px;font-weight:550}.script-items small,.script-items p{color:var(--muted);font-size:11px}.bottom{margin-top:auto;border-top:1px solid var(--border);padding-top:8px}
</style>
