<!-- The account's profile: banner, picture and name, play stats for all time, top songs and
     artists, and the way into Wrapped. -->
<script lang="ts">
  import { api, type Stats } from "../lib/api";
  import Avatar from "../lib/components/Avatar.svelte";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { i18n, t } from "../lib/i18n/index.svelte";
  import { openArtist } from "../lib/navigate";
  import { changeProfileImage, removeProfileImage, renameProfile } from "../lib/profileActions";
  import { history } from "../lib/stores/history.svelte";
  import { menu } from "../lib/stores/menu.svelte";
  import { session } from "../lib/stores/session.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { monthPeriod, yearPeriod } from "../lib/wrapped/cards";

  let stats = $state<Stats | null>(null);
  let failed = $state(false);
  let kind = $state<"month" | "year">("month");
  /** 0 = this month/year, -1 = the one before, ... */
  let offset = $state(0);

  const account = $derived(session.account);
  /** A banner file that couldn't be loaded (e.g. after restoring a backup on another PC). */
  let bannerFailed = $state<string | null>(null);
  const showBanner = $derived(!!account?.banner && account.banner !== bannerFailed);
  const period = $derived(kind === "month" ? monthPeriod(new Date(), offset, i18n.lang) : yearPeriod(new Date(), offset));

  $effect(() => {
    void history.version;
    api.listeningStats(0, Date.now() / 1000 + 60).then(
      (s) => {
        stats = s;
        failed = false;
      },
      () => (failed = true),
    );
  });

  const since = $derived(stats?.firstPlay ? new Date(stats.firstPlay * 1000).toLocaleDateString(i18n.lang, { month: "long", year: "numeric" }) : null);
  const num = (n: number) => n.toLocaleString(i18n.lang);

  function editMenu(e: MouseEvent) {
    menu.show(e, [
      { label: t("profile.changeAvatar"), icon: "image", action: () => changeProfileImage("avatar") },
      { label: t("profile.removeAvatar"), icon: "trash", disabled: !account?.avatar, action: () => removeProfileImage("avatar") },
      { label: t("profile.changeBanner"), icon: "crop", divider: true, action: () => changeProfileImage("banner") },
      { label: t("profile.removeBanner"), icon: "trash", disabled: !account?.banner, action: () => removeProfileImage("banner") },
      { label: t("profile.rename"), icon: "pencil", divider: true, action: renameProfile },
    ]);
  }

  function setKind(k: "month" | "year") {
    kind = k;
    offset = 0;
  }
</script>

{#if account}
  <header>
    <button class="banner" class:empty={!showBanner} aria-label={t("profile.changeBanner")} onclick={() => changeProfileImage("banner")}>
      {#if showBanner}
        <img src={account.banner} alt="" draggable="false" onerror={() => (bannerFailed = account.banner)} />
      {:else}
        <span>{t("profile.addBanner")}</span>
      {/if}
    </button>
    <div class="who">
      <button class="avatar" aria-label={t("profile.changeAvatar")} onclick={() => changeProfileImage("avatar")}>
        <Avatar src={account.avatar} name={account.displayName} size={128} />
      </button>
      <div class="names">
        <span class="kind">{t("profile.kind")}</span>
        <h1>{account.displayName}</h1>
        <div class="email">{account.email}{#if since} · {t("profile.since", { date: since })}{/if}</div>
      </div>
      <Button variant="secondary" icon="pencil" onclick={editMenu}>{t("profile.edit")}</Button>
    </div>
  </header>

  {#if failed}
    <p class="muted">{t("error.generic")}</p>
  {:else if !stats}
    <div class="center"><Spinner size={22} /></div>
  {:else}
    <div class="stats">
      <div><b>{num(stats.minutes)}</b><span>{t("profile.minutes")}</span></div>
      <div><b>{num(stats.plays)}</b><span>{t("profile.plays")}</span></div>
      <div><b>{num(stats.songs)}</b><span>{t("profile.songs")}</span></div>
      <div><b>{num(stats.artists)}</b><span>{t("profile.artists")}</span></div>
    </div>

    <section class="wrapped">
      <div>
        <h2>{t("wrapped.title")}</h2>
        <p>{t("wrapped.lead")}</p>
      </div>
      <div class="picker">
        <Segmented
          label={t("wrapped.title")}
          value={kind}
          options={[
            { value: "month", label: t("wrapped.month") },
            { value: "year", label: t("wrapped.year") },
          ]}
          onchange={setKind} />
        <IconButton icon="back" label={t("wrapped.earlier")} onclick={() => offset--} />
        <span class="period">{period.label}</span>
        <IconButton icon="forward" label={t("wrapped.later")} disabled={offset >= 0} onclick={() => offset++} />
        <Button variant="primary" icon="chart" onclick={() => (ui.wrapped = period)}>{t("wrapped.open")}</Button>
      </div>
    </section>

    {#if stats.topSongs.length}
      <section>
        <h2>{t("profile.topSongs")}</h2>
        <TrackList tracks={stats.topSongs.map((c) => c.track)} compact />
      </section>
    {/if}
    {#if stats.topArtists.length}
      <section>
        <h2>{t("profile.topArtists")}</h2>
        <div class="grid">
          {#each stats.topArtists as a (a.name)}
            <Tile
              title={a.name}
              subtitle={t("wrapped.plays", { n: a.plays, count: num(a.plays) })}
              cover={a.cover}
              round
              icon="user"
              onclick={() => openArtist({ id: null, name: a.name }, "youtube")} />
          {/each}
        </div>
      </section>
    {/if}
  {/if}
{/if}

<style>
  header {
    margin-bottom: var(--s5);
  }
  .banner {
    display: grid;
    place-items: center;
    width: 100%;
    aspect-ratio: 4;
    max-height: 260px;
    overflow: hidden;
    padding: 0;
    border: 0;
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-3);
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .banner.empty:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-2);
  }
  .banner img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .who {
    display: flex;
    align-items: flex-end;
    gap: var(--s5);
    margin: -56px 0 0 var(--s5);
    padding-right: var(--s2);
  }
  .avatar {
    padding: 0;
    border: 4px solid var(--bg);
    border-radius: 50%;
    background: var(--bg);
  }
  .names {
    flex: 1;
    min-width: 0;
    padding-bottom: 6px;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  h1 {
    margin: 2px 0 4px;
    font-size: 40px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }
  .email {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .who :global(.btn) {
    margin-bottom: 10px;
  }
  .center {
    display: grid;
    place-items: center;
    height: 30vh;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--s2);
    margin-bottom: var(--s6);
  }
  .stats div {
    display: grid;
    gap: 2px;
    padding: var(--s4);
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .stats b {
    font-size: var(--fs-2xl);
    font-weight: 750;
    font-variant-numeric: tabular-nums;
  }
  .stats span {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  section {
    margin-bottom: var(--s6);
  }
  h2 {
    margin: 0 0 var(--s3);
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .wrapped {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--s4);
    padding: var(--s4) var(--s5);
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .wrapped h2 {
    margin: 0;
  }
  .wrapped p {
    margin: 2px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .picker {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .period {
    min-width: 130px;
    text-align: center;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 4px;
    margin: 0 -8px;
  }
</style>
