<!-- Three bouncing bars laid over a cover: this is the song that's playing. They sit on the
     cover art itself, so they stay white with a dark wash in both themes. -->
<script lang="ts">
  let { radius = 6 }: { radius?: number } = $props();
</script>

<span class="eq" style:border-radius="{radius}px" aria-hidden="true"><i></i><i></i><i></i></span>

<style>
  .eq {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 2px;
    padding-bottom: 12px;
    background: rgba(0, 0, 0, 0.45);
    animation: wash-in 200ms var(--ease);
  }
  .eq i {
    width: 3px;
    height: 14px;
    border-radius: 2px;
    background: #fff;
    transform-origin: bottom;
    animation: bounce 0.9s ease-in-out infinite;
  }
  .eq i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .eq i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes bounce {
    0%,
    100% {
      transform: scaleY(0.3);
    }
    50% {
      transform: scaleY(1);
    }
  }
  @keyframes wash-in {
    from {
      opacity: 0;
    }
  }
</style>
