<script lang="ts">
  import { Popover } from "bits-ui";
  import type { ModelId, ModelRecord } from "../../bridge/types";

  export let models: ModelRecord[];
  export let value: ModelId | "";
  export let onChange: (next: ModelId) => void;

  let open = false;
  let search = "";
  $: selected = models.find(model => model.id === value);
  $: visible = models.filter(model => `${model.name} ${model.provider}`.toLowerCase().includes(search.toLowerCase())).slice(0, 80);

  function choose(model: ModelRecord) {
    onChange(model.id);
    open = false;
    search = "";
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger class="model-picker-trigger" aria-label="Choose generation model">
    <span>{selected ? `${selected.name} · ${selected.provider}` : "Choose model"}</span><span aria-hidden="true">⌄</span>
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content class="model-picker-content" sideOffset={6} align="start">
      <input type="search" bind:value={search} placeholder="Search models" aria-label="Search models" />
      <div class="model-picker-list" role="listbox" aria-label="Generation models">
        {#each visible as model (model.id)}
          <button type="button" role="option" aria-selected={value === model.id} on:click={() => choose(model)}>
            <strong>{model.name}</strong><small>{model.provider} · {model.status}</small>
          </button>
        {:else}<p>No matching models</p>{/each}
      </div>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

<style>
  :global(.model-picker-trigger){display:flex;align-items:center;justify-content:space-between;gap:12px;width:100%;min-width:0;max-width:290px;padding:8px 10px;overflow:hidden;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text);font-size:12px;text-align:left}
  :global(.model-picker-trigger span:first-child){overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  :global(.model-picker-trigger:hover),:global(.model-picker-trigger[data-state="open"]){background:var(--surface-hover)}
  :global(.model-picker-content){z-index:100;width:min(360px,80vw);padding:8px;border:1px solid var(--border);border-radius:9px;background:var(--surface)}
  :global(.model-picker-content input){width:100%;padding:8px;border:1px solid var(--border);border-radius:7px;background:var(--background);color:var(--text)}
  :global(.model-picker-list){max-height:250px;overflow:auto;padding-top:5px}
  :global(.model-picker-list button){display:flex;flex-direction:column;gap:3px;width:100%;padding:8px;text-align:left;border:0;border-radius:7px;background:none;color:var(--text)}
  :global(.model-picker-list button:hover),:global(.model-picker-list button[aria-selected="true"]){background:var(--surface-hover)}
  :global(.model-picker-list small),:global(.model-picker-list p){color:var(--muted)}
</style>
