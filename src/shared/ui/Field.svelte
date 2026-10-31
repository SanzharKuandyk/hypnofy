<script lang="ts">
  import type { Snippet } from "svelte";
  let {
    label,
    help = "",
    children,
  }: { label: string; help?: string; children: Snippet } = $props();
</script>

<div class="field">
  <div class="field-label">{label}</div>
  {@render children()}
  {#if help}<small class="muted">{help}</small>{/if}
</div>
