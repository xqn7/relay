<script lang="ts">
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import EditLocalDialog from "./EditLocalDialog.svelte";
  import FixMatchDialog from "./FixMatchDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import LikedImportDialog from "./LikedImportDialog.svelte";
  import MergeDialog from "./MergeDialog.svelte";
  import ReportDialog from "./ReportDialog.svelte";
  import SmallDialogs from "./SmallDialogs.svelte";

  const d = $derived(dialogs.current);
</script>

{#if d}
  {#key d}
    {#if d.kind === "prompt" || d.kind === "confirm"}
      <SmallDialogs {d} />
    {:else if d.kind === "import"}
      <ImportDialog url={d.url} />
    {:else if d.kind === "importLiked"}
      <LikedImportDialog />
    {:else if d.kind === "report"}
      <ReportDialog title={d.title} unmatched={d.unmatched} skipped={d.skipped} truncated={d.truncated} />
    {:else if d.kind === "fixMatch"}
      <FixMatchDialog track={d.track} />
    {:else if d.kind === "editLocal"}
      <EditLocalDialog track={d.track} />
    {:else if d.kind === "merge"}
      <MergeDialog playlistId={d.playlistId} />
    {/if}
  {/key}
{/if}
