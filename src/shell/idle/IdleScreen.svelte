<!-- Watches for inactivity: after the chosen minutes without mouse or keyboard input, while music
     plays and the window is in front, the idle screen fades in. Any input fades it out again. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import IdleView from "./IdleView.svelte";

  /** Input events are frequent; the timer only restarts this often. */
  const REARM_THROTTLE_MS = 1000;

  let idle = $state(false);
  let timer = 0;
  let lastArm = 0;

  const inFront = () => document.visibilityState === "visible" && document.hasFocus();

  function arm() {
    clearTimeout(timer);
    lastArm = Date.now();
    timer = window.setTimeout(check, settings.values.idleMinutes * 60_000);
  }

  function check() {
    if (player.playing && inFront()) idle = true;
    else arm();
  }

  function wake() {
    idle = false;
    arm();
  }

  function onInput(e: Event) {
    if (idle) {
      // The input that ends the idle screen only does that: no click or key goes through.
      e.preventDefault();
      e.stopPropagation();
      wake();
    } else if (Date.now() - lastArm > REARM_THROTTLE_MS) arm();
  }

  function onHide() {
    if (!inFront()) idle = false;
    arm();
  }

  // A changed delay applies right away.
  $effect(() => {
    void settings.values.idleMinutes;
    arm();
  });

  onMount(() => {
    const opts = { capture: true, passive: false };
    const events = ["pointermove", "pointerdown", "keydown", "wheel"] as const;
    for (const name of events) window.addEventListener(name, onInput, opts);
    window.addEventListener("blur", onHide);
    document.addEventListener("visibilitychange", onHide);
    return () => {
      clearTimeout(timer);
      for (const name of events) window.removeEventListener(name, onInput, opts);
      window.removeEventListener("blur", onHide);
      document.removeEventListener("visibilitychange", onHide);
    };
  });
</script>

{#if idle}<IdleView />{/if}
