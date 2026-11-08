<script lang="ts">
  import Icon from "./Icon.svelte";

  let { size = 18 }: { size?: number } = $props();
</script>

<span class="spinner"><Icon name="loader" {size} /></span>

<style>
  .spinner {
    display: inline-grid;
    color: var(--text-3);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
