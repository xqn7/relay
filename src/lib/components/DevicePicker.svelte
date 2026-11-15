<!-- Output device menu button: "System default (X)" plus every output Windows has. -->
<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { menu, type MenuItem } from "../stores/menu.svelte";
  import { settings } from "../stores/settings.svelte";
  import { sound } from "../stores/sound.svelte";
  import Icon from "./Icon.svelte";

  const picked = $derived(settings.values.outputDevice);
  const missing = $derived(picked !== "" && !sound.devices.some((d) => d.name === picked));
  const label = $derived(picked === "" ? t("sound.systemDefault") : picked);

  function open(e: MouseEvent) {
    e.stopPropagation();
    const mark = (name: string) => (picked === name ? "check" : "empty");
    const items: MenuItem[] = [
      { label: sound.defaultName ? `${t("sound.systemDefault")} (${sound.defaultName})` : t("sound.systemDefault"), icon: mark(""), action: () => sound.pick("") },
      ...sound.devices.map((d, i) => ({ label: d.name, icon: mark(d.name), divider: i === 0, action: () => sound.pick(d.name) }) satisfies MenuItem),
    ];
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu.show(new MouseEvent("contextmenu", { clientX: r.left, clientY: r.bottom + 4 }), items);
  }
</script>

<button class="picker" title={missing ? t("sound.deviceMissing") : label} onclick={open}>
  <Icon name="speaker" size={15} />
  <span class="truncate">{label}</span>
  {#if missing}<Icon name="alert" size={14} />{/if}
  <Icon name="chevronDown" size={14} />
</button>

<style>
  .picker {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    min-width: 0;
    padding: 6px 8px 6px 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(var(--ink), 0.07);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    transition: background var(--t-fast) var(--ease);
  }
  .picker:hover {
    background: rgba(var(--ink), 0.12);
  }
  .picker :global(svg) {
    flex: none;
  }
</style>
