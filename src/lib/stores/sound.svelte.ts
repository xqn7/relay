// Output device, equalizer, speed and pitch. Each output device remembers its own EQ and
// volume (saved per PC, keyed by the device's name, which stays the same across restarts
// while Chromium's device ids don't have to).
import { BUILT_IN, flatEq, presetGains, type EqState } from "../audio/eq";
import { t } from "../i18n/index.svelte";
import { player } from "./player.svelte";
import { settings, type DeviceProfile } from "./settings.svelte";
import { ui } from "./ui.svelte";

export interface OutputDevice {
  id: string;
  name: string;
}

/** Chromium lists the Windows default twice ("Default - X", "Communications - X"). */
const DEFAULT_PREFIX = /^Default - /;

class SoundStore {
  devices = $state<OutputDevice[]>([]);
  /** The real device behind "System default", by name. */
  defaultName = $state("");
  eq = $state<EqState>(flatEq());
  private attached = false;

  /** The name the current profile is stored under. */
  get activeName(): string {
    return settings.values.outputDevice || this.defaultName;
  }

  async attach(): Promise<void> {
    if (!this.attached) {
      this.attached = true;
      navigator.mediaDevices?.addEventListener("devicechange", () => this.refresh("change"));
      let timer = 0;
      player.onVolumeChange = () => {
        clearTimeout(timer);
        timer = window.setTimeout(() => this.saveProfile(), 800);
      };
    }
    await this.refresh("startup");
    player.engine.setSpeed(settings.values.speed);
    await player.engine.setPitch(settings.values.pitch).catch((e) => console.error("pitch unavailable", e));
  }

  /** Reads the device list; a picked device that went away falls back to the default.
   * "change" = Windows reported a device change; "pick" = the user chose a device. */
  async refresh(why: "startup" | "pick" | "change"): Promise<void> {
    const changed = why === "change";
    const before = this.activeName;
    const all = await navigator.mediaDevices.enumerateDevices().catch(() => []);
    const outputs = all.filter((d) => d.kind === "audiooutput" && d.label);
    this.defaultName = outputs.find((d) => d.deviceId === "default")?.label.replace(DEFAULT_PREFIX, "") ?? "";
    this.devices = outputs.filter((d) => d.deviceId !== "default" && d.deviceId !== "communications").map((d) => ({ id: d.deviceId, name: d.label }));
    const picked = settings.values.outputDevice;
    let sink = picked ? (this.devices.find((d) => d.name === picked)?.id ?? null) : "";
    if (sink == null) {
      // The picked device was unplugged: play through the default instead, keep the choice
      // so it comes back when the device does.
      sink = "";
      if (changed) ui.toast(t("sound.deviceGone", { name: picked }), "speaker");
    }
    await player.engine.setSink(sink).catch((e) => console.error("output device", e));
    if (this.activeName !== before || !changed) this.loadProfile(why === "startup");
    if (changed && this.activeName !== before && settings.values.pauseOnDeviceChange && player.playing) player.pause();
  }

  async pick(name: string): Promise<void> {
    this.saveProfile();
    settings.set("outputDevice", name);
    await this.refresh("pick");
  }

  /** Applies the active device's EQ and (except at startup, where the queue's volume wins) its volume. */
  private loadProfile(startup: boolean): void {
    const profile = settings.values.deviceProfiles[this.activeName];
    const eq = profile?.eq ?? flatEq();
    // A saved preset that was deleted meanwhile keeps its gains as "custom".
    if (eq.preset !== "custom" && !presetGains(eq.preset, settings.values.eqPresets)) eq.preset = "custom";
    this.eq = eq;
    player.engine.setEq(eq.on, eq.gains);
    if (profile && !startup) player.setVolume(profile.volume);
  }

  /** Remembers the EQ and volume for the device in use. */
  saveProfile(): void {
    const name = this.activeName;
    if (!name) return;
    const profiles: Record<string, DeviceProfile> = { ...$state.snapshot(settings.values.deviceProfiles) };
    profiles[name] = { eq: $state.snapshot(this.eq), volume: player.volume };
    settings.set("deviceProfiles", profiles);
  }

  setEq(next: EqState): void {
    this.eq = next;
    player.engine.setEq(next.on, next.gains);
    this.saveProfile();
  }

  setEqOn(on: boolean): void {
    this.setEq({ ...$state.snapshot(this.eq), on });
  }

  choosePreset(id: string): void {
    const gains = presetGains(id, settings.values.eqPresets);
    if (gains) this.setEq({ on: true, preset: id, gains });
  }

  setBand(i: number, db: number): void {
    const gains = [...this.eq.gains];
    gains[i] = Math.round(db * 2) / 2;
    this.setEq({ on: true, preset: "custom", gains });
  }

  savePreset(name: string): void {
    const clean = name.trim().slice(0, 40);
    if (!clean) return;
    const rest = settings.values.eqPresets.filter((p) => p.name !== clean);
    settings.set("eqPresets", [...$state.snapshot(rest), { name: clean, gains: [...this.eq.gains] }]);
    this.setEq({ ...$state.snapshot(this.eq), preset: `user:${clean}` });
  }

  deletePreset(name: string): void {
    settings.set("eqPresets", $state.snapshot(settings.values.eqPresets.filter((p) => p.name !== name)));
    if (this.eq.preset === `user:${name}`) this.setEq({ ...$state.snapshot(this.eq), preset: "custom" });
  }

  presetLabel(id: string): string {
    if (id in BUILT_IN) return t(`eq.preset.${id}` as never);
    if (id.startsWith("user:")) return id.slice(5);
    return t("eq.custom");
  }

  setSpeed(rate: number): void {
    const r = Math.round(rate * 20) / 20;
    settings.set("speed", r);
    player.engine.setSpeed(settings.values.speed);
  }

  setPitch(semitones: number): void {
    settings.set("pitch", Math.round(semitones));
    player.engine.setPitch(settings.values.pitch).catch((e) => {
      console.error("pitch failed", e);
      ui.toast(t("error.generic"), "alert");
    });
  }
}

export const sound = new SoundStore();
