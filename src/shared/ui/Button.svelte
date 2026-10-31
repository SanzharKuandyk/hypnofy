<script lang="ts">
  import { Tooltip } from "bits-ui";
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
    title,
    ...rest
  }: Props = $props();

  const hint = $derived(
    title || (typeof rest["aria-label"] === "string" ? rest["aria-label"] : ""),
  );
</script>

{#snippet content()}
  {#if loading}<span class="spinner" aria-hidden="true"></span>{/if}
  {#if children}{@render children()}{/if}
{/snippet}

{#if hint}
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}<button
          {...props}
          type="button"
          class={`button ${variant} ${className}`}
          disabled={disabled || loading}
          aria-busy={loading}
          {...rest}
          onclick={(event) => {
            if (typeof props.onclick === "function") props.onclick(event);
            rest.onclick?.(event);
          }}>{@render content()}</button
        >{/snippet}
    </Tooltip.Trigger>
    <Tooltip.Portal
      ><Tooltip.Content role="tooltip" class="control-tooltip" sideOffset={8} collisionPadding={12}
        >{hint}</Tooltip.Content
      ></Tooltip.Portal
    >
  </Tooltip.Root>
{:else}
  <button
    type="button"
    class={`button ${variant} ${className}`}
    disabled={disabled || loading}
    aria-busy={loading}
    {...rest}
  >
    {@render content()}
  </button>
{/if}
