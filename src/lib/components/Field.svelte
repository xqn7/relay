<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let {
    label,
    value = $bindable(""),
    type = "text",
    placeholder = "",
    error = "",
    autocomplete,
    autofocus = false,
  }: {
    label: string;
    value?: string;
    type?: "text" | "email" | "password";
    placeholder?: string;
    error?: string;
    autocomplete?: HTMLInputElement["autocomplete"];
    autofocus?: boolean;
  } = $props();

  let reveal = $state(false);
  const id = `f-${Math.random().toString(36).slice(2, 8)}`;
  const inputType = $derived(type === "password" && reveal ? "text" : type);

  function focusOnMount(node: HTMLInputElement) {
    if (autofocus) node.focus();
  }
</script>

<div class="field" class:invalid={!!error}>
  <label for={id}>{label}</label>
  <div class="box">
    <input {id} type={inputType} bind:value {placeholder} {autocomplete} spellcheck="false" aria-invalid={!!error} use:focusOnMount />
    {#if type === "password"}
      <button type="button" class="eye" aria-label={reveal ? t("auth.hidePassword") : t("auth.showPassword")} onclick={() => (reveal = !reveal)}>
        <Icon name={reveal ? "eyeOff" : "eye"} size={16} />
      </button>
    {/if}
  </div>
  {#if error}
    <p class="error"><Icon name="alert" size={14} />{error}</p>
  {/if}
</div>

<style>
  .field {
    display: grid;
    gap: 6px;
  }
  label {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
  }
  .box {
    position: relative;
  }
  input {
    width: 100%;
    height: 42px;
    padding: 0 var(--s3);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    background: var(--field);
    font-size: var(--fs-md);
    outline: none;
    transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  input::placeholder {
    color: var(--text-3);
  }
  input:focus {
    border-color: rgba(255, 255, 255, 0.28);
  }
  .invalid input {
    border-color: rgba(255, 255, 255, 0.5);
  }
  .box:has(.eye) input {
    padding-right: 40px;
  }
  .eye {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
  }
  .eye:hover {
    color: var(--text);
  }
  .error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text);
  }
</style>
