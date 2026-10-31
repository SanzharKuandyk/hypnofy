<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";

  interface Props extends HTMLButtonAttributes {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    loading?: boolean;
    children?: Snippet;
  }

  let {
    variant = "secondary",
    loading = false,
    children,
    class: className = "",
    disabled,
    ...rest
  }: Props = $props();
</script>

<button
  type="button"
  class={`button ${variant} ${className}`}
  disabled={disabled || loading}
  aria-busy={loading}
  {...rest}
>
  {#if loading}<span class="spinner" aria-hidden="true"></span>{/if}
  {#if children}{@render children()}{/if}
</button>
