<script lang="ts">
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import ConnectionDialog from "./ConnectionDialog.svelte";
  import DesktopDialogs from "./DesktopDialogs.svelte";
  import EditLocalDialog from "./EditLocalDialog.svelte";
  import FixMatchDialog from "./FixMatchDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import LikedImportDialog from "./LikedImportDialog.svelte";
  import MergeDialog from "./MergeDialog.svelte";
  import ReportDialog from "./ReportDialog.svelte";
  import SmallDialogs from "./SmallDialogs.svelte";
  import VersionsDialog from "./VersionsDialog.svelte";

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
    {:else if d.kind === "connection"}
      <ConnectionDialog />
    {:else if d.kind === "problem"}
      {#await import("./ProblemDialog.svelte") then m}<m.default />{/await}
    {:else if d.kind === "update" || d.kind === "hardware" || d.kind === "changelog"}
      <DesktopDialogs {d} />
    {:else if d.kind === "versions"}
      <VersionsDialog track={d.track} playlist={d.playlist} />
    {:else if d.kind === "crop"}
      {#await import("./CropDialog.svelte") then m}<m.default {d} />{/await}
    {/if}
  {/key}
{/if}
