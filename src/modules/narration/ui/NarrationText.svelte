<script lang="ts">
  import { narrationParts } from "../../../domain/pause-cues";

  let { text }: { text: string } = $props();
  const parts = $derived.by(() => {
    try {
      return narrationParts(text);
    } catch {
      return [{ kind: "speech" as const, text, from: 0, to: text.length }];
    }
  });
</script>

<span style="white-space:pre-wrap"
  >{#each parts as part (part.from)}{#if part.kind === "speech"}{part.text}{:else}<span
        class="pause-chip">◷ Pause · {part.durationMs / 1000}s</span
      >{/if}{/each}</span
>
