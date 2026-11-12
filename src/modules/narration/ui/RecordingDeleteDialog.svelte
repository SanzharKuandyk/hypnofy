<script lang="ts">
  import { Dialog, Button } from "../../../shared/ui";
  import { api } from "../../../shared/rpc/client";

  let {
    id,
    open = $bindable(false),
    onchanged,
    onremoved = () => {},
  }: {
    id: string | null;
    open?: boolean;
    onchanged: () => Promise<void>;
    onremoved?: () => void;
  } = $props();
  let busy = $state(false);
  let error = $state("");

  $effect(() => {
    if (!open) error = "";
  });

  async function remove(): Promise<void> {
    if (!id) return;
    busy = true;
    error = "";
    try {
      const result = await api.narration.remove(id);
      await onchanged();
      if (result.kind === "pending") {
        error = result.message;
        return;
      }
      open = false;
      onremoved();
    } catch (value) {
      error = value instanceof Error ? value.message : "The recording could not be deleted.";
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  bind:open
  title="Delete recording?"
  description="Deletes this saved recording from Hypnofy. Your script, background sounds and exported WAV files are kept."
>
  {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
  <div class="actions">
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
        error = "";
      }}>Cancel</Button
    ><Button variant="danger" loading={busy} onclick={remove}
      >{error ? "Retry deletion" : "Delete recording"}</Button
    >
  </div>
</Dialog>
