<script lang="ts">
  import { Tooltip } from "bits-ui";
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<Tooltip.Provider delayDuration={350} skipDelayDuration={150} ignoreNonKeyboardFocus
  >{@render children()}</Tooltip.Provider
>
