<script lang="ts">
  import { Dialog } from "bits-ui";
  import type { Snippet } from "svelte";

  let {
    open = $bindable(false),
    title,
    description = "",
    required = false,
    children,
    wide = false,
  }: {
    open?: boolean;
    title: string;
    description?: string;
    required?: boolean;
    children: Snippet;
    wide?: boolean;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay class="modal-backdrop" />
    <Dialog.Content
      class={`modal ${wide ? "wide" : ""}`}
      onEscapeKeydown={(event) => {
        if (required) event.preventDefault();
      }}
      onInteractOutside={(event) => {
        if (required) event.preventDefault();
      }}
    >
      <Dialog.Title class="modal-title">{title}</Dialog.Title>
      {#if description}<Dialog.Description class="muted">{description}</Dialog.Description>{/if}
      {@render children()}
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
