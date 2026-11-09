<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let { value = $bindable(""), placeholder, width = 220 }: { value?: string; placeholder: string; width?: number } = $props();
</script>

<label class="search" style:width="{width}px">
  <Icon name="search" size={15} />
  <input bind:value {placeholder} spellcheck="false" onkeydown={(e) => e.key === "Escape" && (value = "")} />
  {#if value}
    <button type="button" aria-label={t("search.clearInput")} onclick={() => (value = "")}><Icon name="close" size={13} /></button>
  {/if}
</label>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 8px 0 12px;
    border-radius: var(--r-md);
    background: var(--glass-strong);
    color: var(--text-3);
    transition: background var(--t-fast) var(--ease);
  }
  .search:focus-within {
    background: var(--glass-raised);
    color: var(--text-2);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  input::placeholder {
    color: var(--text-3);
  }
  button {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--hover);
    color: var(--text-2);
  }
</style>
