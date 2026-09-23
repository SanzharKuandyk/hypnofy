<script lang="ts">
  import { Select } from "bits-ui";

  export type SelectValue = string | number | null;
  export type SelectItem = { value: SelectValue; label: string; detail?: string };

  let {
    value = $bindable(),
    items,
    placeholder = "Select",
    ariaLabel,
    onChange,
    disabled = false,
  }: {
    value?: SelectValue;
    items: SelectItem[];
    placeholder?: string;
    ariaLabel: string;
    onChange?: (value: SelectValue) => void;
    disabled?: boolean;
  } = $props();

  let internalValue = $state(String(value ?? ""));

  $effect(() => {
    internalValue = String(value ?? "");
  });

  function commit(next: string) {
    internalValue = next;
    value = items.find(item => String(item.value ?? "") === next)?.value ?? next;
    onChange?.(value);
  }
</script>

<Select.Root type="single" items={items.map(item => ({ value: String(item.value ?? ""), label: item.label }))} value={internalValue} onValueChange={commit} {disabled}>
  <Select.Trigger class="app-select-trigger" aria-label={ariaLabel}>
    <Select.Value {placeholder} />
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4" /></svg>
  </Select.Trigger>
  <Select.Portal>
    <Select.Content class="app-select-content" sideOffset={6} align="start">
      <Select.Viewport class="app-select-viewport">
        {#each items as item (String(item.value ?? ""))}
          <Select.Item class="app-select-item" value={String(item.value ?? "")} label={item.label}>
            {#snippet children({ selected })}
              <span class="app-select-copy"><strong>{item.label}</strong>{#if item.detail}<small>{item.detail}</small>{/if}</span>
              {#if selected}<svg class="app-select-check" viewBox="0 0 16 16" aria-hidden="true"><path d="m3 8 3 3 7-7" /></svg>{/if}
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>

<style>
  :global(.app-select-trigger) {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    width: 100%; min-height: 34px; padding: 0 10px; border: 1px solid var(--border);
    border-radius: 8px; color: var(--text); background: var(--surface); cursor: pointer;
    text-align: left; font-size: 12px;
  }
  :global(.app-select-trigger:hover), :global(.app-select-trigger[data-state="open"]) { background: var(--surface-hover) }
  :global(.app-select-trigger svg) { width: 15px; height: 15px; fill: none; stroke: var(--muted); stroke-width: 1.5 }
  :global(.app-select-content) {
    z-index: 100; min-width: var(--bits-select-anchor-width); max-height: 280px; overflow: hidden;
    border: 1px solid var(--border); border-radius: 8px; background: var(--surface);
  }
  :global(.app-select-viewport) { padding: 5px; overflow: auto }
  :global(.app-select-item) {
    display: flex; align-items: center; gap: 12px; min-height: 38px; padding: 7px 9px;
    border-radius: 8px; outline: 0; color: #c9c4cd; cursor: pointer; user-select: none;
  }
  :global(.app-select-item[data-highlighted]) { color: var(--text); background: var(--surface-hover) }
  :global(.app-select-item[data-selected]) { color: var(--text); background: var(--surface-hover) }
  :global(.app-select-copy) { display: flex; flex: 1; flex-direction: column; gap: 2px }
  :global(.app-select-copy strong) { font-size: 12px; font-weight: 550 }
  :global(.app-select-copy small) { color: var(--muted); font-size: 10px }
  :global(.app-select-check) { width: 15px; height: 15px; fill: none; stroke: var(--accent); stroke-width: 1.7 }
</style>
