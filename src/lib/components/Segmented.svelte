<script lang="ts" generics="T extends string | number">
  let {
    value,
    options,
    label,
    onchange,
    size = "md",
  }: { value: T; options: { value: T; label: string }[]; label: string; onchange: (v: T) => void; size?: "md" | "lg" } = $props();
</script>

<div class="seg {size}" role="radiogroup" aria-label={label}>
  {#each options as opt (opt.value)}
    <button role="radio" aria-checked={opt.value === value} class:on={opt.value === value} onclick={() => onchange(opt.value)}>{opt.label}</button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--r-md);
    background: rgba(0, 0, 0, 0.32);
  }
  button {
    height: 28px;
    padding: 0 var(--s3);
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  button:hover {
    color: var(--text);
  }
  .on {
    background: var(--glass-raised);
    color: var(--text);
  }
  .lg {
    display: flex;
    padding: 4px;
  }
  .lg button {
    flex: 1;
    height: 36px;
  }
  .lg .on {
    background: var(--primary);
    color: var(--on-primary);
  }
</style>
