<script lang="ts">
  let { checked, label, onchange }: { checked: boolean; label: string; onchange: (v: boolean) => void } = $props();
</script>

<button type="button" class="toggle" class:on={checked} role="switch" aria-checked={checked} aria-label={label} onclick={() => onchange(!checked)}>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 38px;
    height: 22px;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: var(--r-full);
    background: rgba(var(--ink), 0.16);
    transition: background var(--t) var(--ease);
  }
  .toggle.on {
    background: var(--toggle-on);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
    transition: transform var(--t) var(--ease);
  }
  .on .knob {
    transform: translateX(16px);
  }
</style>
