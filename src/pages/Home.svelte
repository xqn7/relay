<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import EmptyState from "./EmptyState.svelte";
</script>

<h1 class="page-title">{t("home.title")}</h1>
<EmptyState icon="home" title={t("home.emptyTitle")} body={t("home.emptyBody")}>
  <Button variant="primary" icon="search" onclick={() => (ui.searchOpen = true)}>{t("home.emptyCta")}</Button>
</EmptyState>

