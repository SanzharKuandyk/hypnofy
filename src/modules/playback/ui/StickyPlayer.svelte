<script lang="ts">
  import { Play, Pause, Square, Volume2, Clock, Download } from "@lucide/svelte";
  import { Button } from "../../../shared/ui";
  import type { PlaybackState, RenderRecord } from "../../../domain";

  let {
    state,
    render,
    onplay,
    oncontrol,
    onexport,
  }: {
    state: PlaybackState | null;
    render: Pick<RenderRecord, "id" | "durationMs"> | undefined;
    onplay: () => void;
    oncontrol: (operation: string, value?: number) => void;
    onexport: () => void;
  } = $props();

  const loaded = $derived(!!render && state?.renderId === render.id);

  function time(ms: number): string {
    return `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
  }
</script>

<div class="player" aria-label="Audio player">
  <div class="player-main">
    <Button
      variant="primary"
      class="icon"
      aria-label={loaded && state?.playing ? "Pause playback" : "Play narration"}
      disabled={!render}
      onclick={onplay}
      >{#if loaded && state?.playing}<Pause size={18} />{:else}<Play size={18} />{/if}</Button
    ><Button
      variant="ghost"
      class="icon"
      aria-label="Stop playback"
      disabled={!loaded}
      onclick={() => oncontrol("stop")}><Square size={16} /></Button
    ><span class="caption"
      >{time(loaded ? (state?.positionMs ?? 0) : 0)} / {time(render?.durationMs ?? 0)}</span
    ><span class="render"
      ><Button disabled={!render} onclick={onexport}><Download size={15} />Export WAV</Button></span
    >
  </div>
  <input
    aria-label="Playback position"
    type="range"
    min="0"
    max={Math.max(1, render?.durationMs ?? 0)}
    value={loaded ? (state?.positionMs ?? 0) : 0}
    disabled={!loaded}
    onchange={(event) => oncontrol("seek", Number(event.currentTarget.value))}
  />
  <div class="player-detail">
    <Volume2 size={15} /><input
      aria-label="Narration volume"
      type="range"
      min="0"
      max="1"
      step=".01"
      value={state?.volume ?? 1}
      style="width:90px"
      onchange={(event) => oncontrol("volume", Number(event.currentTarget.value))}
    /><span class="extra">Narration</span><Clock size={14} /><select
      aria-label="Sleep timer"
      style="padding:2px 6px"
      onchange={(event) => oncontrol("sleep", Number(event.currentTarget.value))}
      ><option value="0">Sleep timer off</option><option value="10">10 minutes</option><option
        value="20">20 minutes</option
      ><option value="30">30 minutes</option><option value="60">60 minutes</option></select
    >{#if state?.sleepRemainingMs}<span>{time(state.sleepRemainingMs)} left</span
      >{/if}{#if state?.error}<span role="alert">{state.error}</span>{/if}
  </div>
</div>
