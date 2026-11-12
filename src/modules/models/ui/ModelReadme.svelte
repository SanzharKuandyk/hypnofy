<script lang="ts">
  let { source }: { source: string } = $props();

  interface ReadmeBlock {
    readonly kind: "heading" | "text" | "code";

    readonly text: string;
  }

  function blocks(markdown: string): ReadmeBlock[] {
    const body = markdown.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, "");
    return body.split(/(```[\s\S]*?```)/g).flatMap<ReadmeBlock>((part) => {
      if (part.startsWith("```"))
        return [
          { kind: "code" as const, text: part.replace(/^```[^\n]*\n?/, "").replace(/```$/, "") },
        ];
      return part
        .replace(/<(script|style|iframe)\b[^>]*>[\s\S]*?<\/\1>/gi, "")
        .replace(/<!--[\s\S]*?-->/g, "")
        .replace(/<[^>]*>/g, "")
        .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
        .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        .replace(/\*\*([^*]+)\*\*/g, "$1")
        .split(/\n\s*\n/)
        .map((text) => ({
          kind: /^\s*#{1,6}\s/.test(text) ? ("heading" as const) : ("text" as const),
          text: text.trim().replace(/^#{1,6}\s+/, ""),
        }))
        .filter((block) => block.text.length > 0);
    });
  }
  const content = $derived(blocks(source));
</script>

<article class="hub-readme" aria-label="Model card">
  {#each content as block, index (index)}{#if block.kind === "heading"}<h3>
        {block.text}
      </h3>{:else if block.kind === "code"}<pre><code>{block.text}</code></pre>{:else}<p>
        {block.text}
      </p>{/if}{/each}
</article>
