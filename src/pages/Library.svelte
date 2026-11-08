<script lang="ts">
  import { t } from "../lib/i18n/index.svelte";
  import EmptyState from "./EmptyState.svelte";
</script>

<h1 class="page-title">{t("library.title")}</h1>
<EmptyState icon="library" title={t("library.soonTitle")} body={t("library.soonBody")} />

