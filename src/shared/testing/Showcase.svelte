<script lang="ts">
  import { Button, Badge, Field, Dialog } from "../ui";

  let open = $state(false);
  let text = $state("");
</script>

<main class="page">
  <header>
    <h1>Hypnofy components</h1>
    <p class="muted">Development preview · /?showcase</p>
  </header>
  <section class="card">
    <h2>Actions</h2>
    <div class="row">
      <Button variant="primary">Primary</Button><Button>Secondary</Button><Button variant="ghost"
        >Quiet</Button
      ><Button variant="danger">Delete</Button><Button disabled>Unavailable</Button><Button loading
        >Working</Button
      >
    </div>
  </section>
  <section class="card">
    <h2>Fields & feedback</h2>
    <Field label="Title" help="A short hint that explains the field."
      ><input aria-label="Preview title" bind:value={text} placeholder="A quiet evening" /></Field
    >
    <div class="row">
      <Badge tone="ready">Ready</Badge><Badge tone="warning">Needs verification</Badge><Badge
        tone="error">Unavailable</Badge
      >
    </div>
    <p class="inline-error">A recoverable error keeps the user’s work available.</p>
    <progress class="progress" value=".6" max="1"></progress>
  </section>
  <section class="card">
    <h2>Dialogs</h2>
    <Button onclick={() => (open = true)}>Open dialog</Button>
  </section>
</main>
<Dialog
  bind:open
  title="A focused conversation"
  description="Keyboard focus stays inside this dialog."
  ><Field label="Name"><input aria-label="Preview collection name" /></Field>
  <div class="actions">
    <Button onclick={() => (open = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() => (open = false)}>Continue</Button
    >
  </div></Dialog
>
