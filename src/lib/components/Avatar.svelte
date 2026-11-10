<!-- The account's profile picture, or its first letter when there's none (or the file is gone). -->
<script lang="ts">
  let { src, name, size }: { src: string | null; name: string; size: number } = $props();
  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });
</script>

<div class="avatar" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.42)}px">
  {#if src && !failed}
    <img {src} alt="" draggable="false" onerror={() => (failed = true)} />
  {:else}
    {name.slice(0, 1).toUpperCase()}
  {/if}
</div>

<style>
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.14);
    font-weight: 700;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
