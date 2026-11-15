<!-- Header of every song list page: playlist, Liked Songs, Local music, album. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { coverDrag, openCoverMenu } from "../lib/coverActions";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import type { IconName } from "../lib/icons";

  let {
    kind,
    title,
    cover = null,
    icon = "list",
    chips = [],
    subtitle,
    actions,
    tools,
  }: {
    kind: string;
    title: string;
    cover?: string | null;
    icon?: IconName;
    chips?: { text: string; icon?: IconName; onclick?: () => void }[];
    subtitle?: Snippet;
    actions?: Snippet;
    tools?: Snippet;
  } = $props();
</script>

<header>
  <div class="art">
    {#if cover}
      <div class="cover" role="img" aria-label={title} oncontextmenu={(e) => openCoverMenu(e, cover, title)} use:coverDrag={{ url: cover, name: title }}>
        <Cover src={cover} size={168} radius={12} />
      </div>
    {:else}
      <div class="icon"><Icon name={icon} size={56} strokeWidth={1.6} /></div>
    {/if}
  </div>
  <div class="info">
    <span class="kind">{kind}</span>
    <h1 title={title}>{title}</h1>
    {#if subtitle}<div class="subtitle">{@render subtitle()}</div>{/if}
    {#if chips.length}
      <div class="chips">
        {#each chips as c, i (i)}
          {#if c.onclick}
            <button class="chip action" onclick={c.onclick}>{#if c.icon}<Icon name={c.icon} size={13} />{/if}{c.text}</button>
          {:else}
            <span class="chip">{#if c.icon}<Icon name={c.icon} size={13} />{/if}{c.text}</span>
          {/if}
        {/each}
      </div>
    {/if}
    <div class="bottom">
      <div class="actions">{#if actions}{@render actions()}{/if}</div>
      <div class="tools">{#if tools}{@render tools()}{/if}</div>
    </div>
  </div>
</header>

<style>
  header {
    display: flex;
    gap: var(--s5);
    align-items: flex-end;
    margin-bottom: var(--s5);
  }
  .art {
    flex: none;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
    border-radius: 12px;
    animation: cover-in 380ms var(--ease) both;
  }
  @keyframes cover-in {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(4px);
    }
  }
  :global(html[data-quality="low"]) .art {
    animation: none;
  }
  .cover {
    cursor: grab;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 168px;
    height: 168px;
    border-radius: 12px;
    background: rgba(var(--ink), 0.08);
    color: rgba(var(--ink), 0.82);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  h1 {
    margin: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--fs-3xl);
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .subtitle {
    font-size: var(--fs-md);
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--glass-strong);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-2);
  }
  .chip.action {
    color: var(--text);
    cursor: pointer;
  }
  .chip.action:hover {
    background: var(--glass-raised);
  }
  .bottom {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .actions,
  .tools {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
</style>
