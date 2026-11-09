// The playback position, updated every frame while music plays, for views that move with the
// music. Only runs while the component using it is on screen; when paused it just follows
// seeks. Call during component setup.
import { untrack } from "svelte";
import { player } from "../stores/player.svelte";

export function livePosition(): { readonly ms: number } {
  let ms = $state(0);
  $effect(() => {
    if (!player.playing) {
      ms = player.positionMs;
      return;
    }
    let raf = 0;
    const tick = () => {
      ms = player.livePositionMs();
      raf = requestAnimationFrame(tick);
    };
    untrack(tick);
    return () => cancelAnimationFrame(raf);
  });
  return {
    get ms() {
      return ms;
    },
  };
}
