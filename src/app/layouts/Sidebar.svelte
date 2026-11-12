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
    Music2,
  } from "@lucide/svelte";
  import { Button, Input } from "../../shared/ui";
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
    <Search size={15} /><Input
      aria-label="Search your library"
      placeholder="Search anything…"
      value={w.query}
      oninput={(value) => {
        w.query = value;
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
    <Button
      variant="ghost"
      title="All scripts"
      class={`nav-item${w.route.kind === "library" && w.route.view === "all" && !w.route.collectionId ? " active" : ""}`}
      onclick={() => navigate({ kind: "library", view: "all", collectionId: null })}
      ><Files /><span class="label">All scripts</span></Button
    >
    <Button
      variant="ghost"
      title="Favorites"
      class={`nav-item${w.route.kind === "library" && w.route.view === "favorites" ? " active" : ""}`}
      onclick={() => navigate({ kind: "library", view: "favorites", collectionId: null })}
      ><Star /><span class="label">Favorites</span></Button
    >
    <Button
      variant="ghost"
      title="Trash"
      class={`nav-item${w.route.kind === "library" && w.route.view === "trash" ? " active" : ""}`}
      onclick={() => navigate({ kind: "library", view: "trash", collectionId: null })}
      ><Trash2 /><span class="label">Trash</span></Button
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
    <Button
      variant="ghost"
      class={`nav-item${w.route.kind === "sounds" ? " active" : ""}`}
      title="Sound library"
      onclick={() => navigate({ kind: "sounds" })}
      ><Music2 /><span class="label">Sound library</span></Button
    >
    <Button
      variant="ghost"
      class={`nav-item${w.route.kind === "models" ? " active" : ""}`}
      title="Models & voices"
      onclick={() => navigate({ kind: "models" })}
      ><Box /><span class="label">Models & voices</span></Button
    >
    <Button
      variant="ghost"
      class={`nav-item${w.route.kind === "settings" ? " active" : ""}`}
      title="Settings"
      onclick={() => navigate({ kind: "settings", section: "appearance" })}
      ><Settings /><span class="label">Settings</span></Button
    >
    <div class="local-status"><ShieldCheck size={16} />Your library stays on this device</div>
  </div>
</aside>
