<script lang="ts">
  let { checked, label, onchange }: { checked: boolean; label: string; onchange: (v: boolean) => void } = $props();
</script>

<button class="toggle" class:on={checked} role="switch" aria-checked={checked} aria-label={label} onclick={() => onchange(!checked)}>
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
    background: rgba(255, 255, 255, 0.16);
    transition: background var(--t) var(--ease);
  }
  .toggle.on {
    background: rgba(255, 255, 255, 0.42);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: transform var(--t) var(--ease);
  }
  .on .knob {
    transform: translateX(16px);
  }
</style>
