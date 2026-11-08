<script lang="ts">
  import { filledIcons, icons, type IconName } from "../icons";

  let { name, size = 20, strokeWidth = 2 }: { name: IconName; size?: number; strokeWidth?: number } = $props();
  const filled = $derived(filledIcons.has(name));
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled ? "currentColor" : "none"}
  stroke={filled ? "none" : "currentColor"}
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true">{@html icons[name]}</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
