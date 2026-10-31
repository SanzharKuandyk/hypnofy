<script lang="ts">
  import {
    Moon,
    Search,
    Plus,
    Files,
    Star,
    Trash2,
    Folder,
    Box,
    Settings,
    ShieldCheck,
  } from "@lucide/svelte";
  import { Button } from "../../shared/ui";
  import type { Workspace } from "../workflows/workspace.svelte";
  import type { Route } from "../routes";

  let {
    workspace: w,
    onnavigate = () => {},
    oncollection,
  }: { workspace: Workspace; onnavigate?: () => void; oncollection: () => void } = $props();

  function navigate(route: Route): void {
    void w.action(async () => {
      await w.navigate(route);
      onnavigate();
    });
  }
</script>

<aside class="sidebar" aria-label="Navigation">
  <button
    class="brand"
    onclick={() => navigate({ kind: "library", view: "all", collectionId: null })}
    ><Moon size={26} />Hypnofy</button
  >
  <div class="sidebar-search">
    <Search size={15} /><input
      aria-label="Search your library"
      placeholder="Search anything…"
      value={w.query}
      oninput={(event) => {
        w.query = event.currentTarget.value;
      }}
      onkeydown={(event) => {
        if (event.key === "Enter") navigate({ kind: "library", view: "all", collectionId: null });
      }}
    />
  </div>
  <Button
    variant="primary"
    onclick={() =>
      w.action(async () => {
        await w.createScript();
        onnavigate();
      })}><Plus size={17} />New script</Button
  >
  <nav class="nav-group">
    <button
      title="All scripts"
      class:active={w.route.kind === "library" && w.route.view === "all" && !w.route.collectionId}
      class="nav-item"
      onclick={() => navigate({ kind: "library", view: "all", collectionId: null })}
      ><Files /><span class="label">All scripts</span></button
    >
    <button
      title="Favorites"
      class:active={w.route.kind === "library" && w.route.view === "favorites"}
      class="nav-item"
      onclick={() => navigate({ kind: "library", view: "favorites", collectionId: null })}
      ><Star /><span class="label">Favorites</span></button
    >
    <button
      title="Trash"
      class:active={w.route.kind === "library" && w.route.view === "trash"}
      class="nav-item"
      onclick={() => navigate({ kind: "library", view: "trash", collectionId: null })}
      ><Trash2 /><span class="label">Trash</span></button
    >
  </nav>
  <div class="sidebar-heading">
    Collections<Button
      variant="ghost"
      class="icon"
      aria-label="Create collection"
      onclick={oncollection}><Plus size={15} /></Button
    >
  </div>
  <nav class="collection-nav nav-group">
    {#each w.collections as collection (collection.id)}<button
        class="nav-item"
        class:active={w.route.kind === "library" && w.route.collectionId === collection.id}
        onclick={() => navigate({ kind: "library", view: "all", collectionId: collection.id })}
        ><Folder /><span class="label">{collection.name}</span><span class="count"
          >{collection.scriptCount}</span
        ></button
      >{/each}
  </nav>
  <div class="sidebar-footer">
    <button
      class="nav-item"
      class:active={w.route.kind === "models"}
      title="Models & voices"
      onclick={() => navigate({ kind: "models" })}
      ><Box /><span class="label">Models & voices</span></button
    >
    <button
      class="nav-item"
      class:active={w.route.kind === "settings"}
      title="Settings"
      onclick={() => navigate({ kind: "settings", section: "appearance" })}
      ><Settings /><span class="label">Settings</span></button
    >
    <div class="local-status"><ShieldCheck size={16} />Your library stays on this device</div>
  </div>
</aside>
