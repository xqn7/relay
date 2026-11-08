<script lang="ts">
  import { t } from "../lib/i18n/index.svelte";
  import EmptyState from "./EmptyState.svelte";

  let { name }: { name: string } = $props();
</script>

<h1 class="page-title">{name}</h1>
<EmptyState icon="user" title={t("artist.soon")} />

