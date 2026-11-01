// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { invoke } from "@tauri-apps/api/core";

/** What the phone's player reports (`NativeAudio.kt`). */
export type NativeEvent =
  | { t: "state"; id: number; seq: number; pos: number; dur: number; buf: number; pwr: boolean; playing: boolean; ready: boolean; ended: boolean }
  | { t: "error"; id: number; code: number; msg?: string }
  | { t: "advanced"; from: number; to: number };

export type NativeSend = (op: string, args: Record<string, unknown>) => Promise<unknown>;

/** The song the phone's player starts by itself when the current one ends. */
export type Upcoming = {
  key: string;
  url: string;
  remote: boolean;
  deviceId: string | null;
  meta: { title: string; artist: string; album: string; coverPath: string | null; favorite: boolean; duration: number };
};

export type Adopted = { url: string; remote: boolean; deviceId: string | null };

type Item = {
  id: number;
  currentTime: number;
  duration: number;
  bufferedEnd: number;
  paused: boolean;
  ended: boolean;
  ready: boolean;
  playing: boolean;
  error: { code: number } | null;
};

const fresh = (id: number): Item => ({
  id,
  currentTime: 0,
  duration: Number.NaN,
  bufferedEnd: 0,
  paused: true,
  ended: false,
  ready: false,
  playing: false,
  error: null,
});

/**
 * Stands in for the HTMLAudioElement on Android. The web view's own audio stops or starves once the
 * screen is off, so the sound comes from an ExoPlayer in the app, and this object turns its reports
 * back into the media events the playback engine already listens to. The engine is the same as on
 * desktop; only its element differs.
 *
 * When a song ends, the phone's player starts the song lined up with `setUpcoming` by itself. The
 * engine then claims it (`adopt`) instead of loading it again, so a slow or held-back script never
 * stops the music between songs.
 */
export class NativeAudio extends EventTarget {
  preload = "auto";
  private src_: string | null = null;
  private item: Item = fresh(0);
  private nextId = 0;
  private seq = 0;
  private chain: Promise<unknown> = Promise.resolve();
  private volume_ = 1;
  private muted_ = false;
  private upcoming: (Upcoming & { id: number }) | null = null;
  /** The song the phone's player moved on to by itself, waiting for the engine to claim it. */
  private advanced: (Upcoming & { item: Item }) | null = null;

  constructor(private readonly sendRaw: NativeSend) {
    super();
  }

  /** Commands go one after another, so "load" always arrives before the "play" that follows it. */
  private send(op: string, args: Record<string, unknown> = {}): void {
    const seq = ++this.seq;
    this.chain = this.chain.then(() => this.sendRaw(op, { ...args, op, seq })).catch(() => {});
  }

  private fire(name: string): void {
    this.dispatchEvent(new Event(name));
  }

  private later(...names: string[]): void {
    const id = this.item.id;
    queueMicrotask(() => {
      if (this.item.id !== id) return;
      names.forEach((name) => this.fire(name));
    });
  }

  get src(): string {
    return this.src_ ?? "";
  }

  set src(value: string) {
    this.src_ = value;
  }

  hasAttribute(name: string): boolean {
    return name === "src" && this.src_ !== null;
  }

  removeAttribute(name: string): void {
    if (name !== "src") return;
    this.src_ = null;
    this.item = fresh(0);
    this.send("stop");
  }

  load(): void {
    if (this.src_ === null) return;
    this.item = fresh(++this.nextId);
    this.send("load", { id: this.item.id, url: this.src_ });
  }

  play(): Promise<void> {
    if (!this.src_ || this.item.id === 0) return Promise.reject(new DOMException("no source", "NotSupportedError"));
    const wasPaused = this.item.paused;
    this.item.paused = false;
    if (this.item.ended) {
      this.item.ended = false;
      this.item.currentTime = 0;
    }
    this.send("play", { id: this.item.id });
    if (wasPaused) this.later("play");
    return Promise.resolve();
  }

  pause(): void {
    if (this.item.id === 0) return;
    const wasPaused = this.item.paused;
    this.item.paused = true;
    this.send("pause", { id: this.item.id });
    if (!wasPaused) this.later("pause");
  }

  get paused(): boolean {
    return this.item.paused;
  }

  get ended(): boolean {
    return this.item.ended;
  }

  get duration(): number {
    return this.item.duration;
  }

  get currentTime(): number {
    return this.item.currentTime;
  }

  set currentTime(value: number) {
    if (this.item.id === 0) return;
    this.item.currentTime = value;
    this.item.ended = false;
    this.send("seek", { id: this.item.id, sec: value });
  }

  get error(): { code: number } | null {
    return this.item.error;
  }

  get buffered(): { length: number; start: (index: number) => number; end: (index: number) => number } {
    const end = this.item.bufferedEnd;
    return { length: end > 0 ? 1 : 0, start: () => 0, end: () => end };
  }

  get volume(): number {
    return this.volume_;
  }

  set volume(value: number) {
    this.volume_ = value;
    this.sendVolume();
  }

  get muted(): boolean {
    return this.muted_;
  }

  set muted(value: boolean) {
    this.muted_ = value;
    this.sendVolume();
  }

  private sendVolume(): void {
    this.send("volume", { v: this.muted_ ? 0 : this.volume_ });
  }

  /** Lines up the song that follows, or clears it with null. */
  setUpcoming(next: Upcoming | null): void {
    if (!next) {
      if (!this.upcoming) return;
      this.upcoming = null;
      this.send("next", {});
      return;
    }
    if (this.upcoming && this.upcoming.key === next.key && this.upcoming.url === next.url) return;
    this.upcoming = { ...next, id: ++this.nextId };
    this.send("next", { id: this.upcoming.id, url: next.url, ...next.meta });
  }

  get upcomingKey(): string | null {
    return this.upcoming?.key ?? null;
  }

  /** Claims the song the phone's player already moved on to, if it is `key`. Call `replay` once listeners are bound. */
  adopt(key: string): Adopted | null {
    const advanced = this.advanced;
    this.advanced = null;
    if (!advanced || advanced.key !== key) return null;
    this.item = advanced.item;
    this.src_ = advanced.url;
    return { url: advanced.url, remote: advanced.remote, deviceId: advanced.deviceId };
  }

  /** Sends the events a fresh listener missed for the adopted song. */
  replay(): void {
    const item = this.item;
    queueMicrotask(() => {
      if (this.item !== item) return;
      if (Number.isFinite(item.duration) && item.duration > 0) this.fire("loadedmetadata");
      if (Number.isFinite(item.duration) && item.duration > 0) this.fire("durationchange");
      if (item.ready) this.fire("canplay");
      if (!item.paused) this.fire("play");
      if (item.playing) this.fire("playing");
      if (item.error) this.fire("error");
      else if (item.ended) {
        this.fire("pause");
        this.fire("ended");
      }
    });
  }

  /** A report from the phone's player. */
  receive(event: NativeEvent): void {
    if (event.t === "advanced") {
      const upcoming = this.upcoming;
      this.upcoming = null;
      // Only a move on from the song shown here counts; anything else was already replaced by the page.
      if (upcoming && upcoming.id === event.to && event.from === this.item.id && this.item.id !== 0) {
        const item = fresh(event.to);
        item.paused = false;
        this.advanced = { ...upcoming, item };
      } else {
        this.advanced = null;
      }
      if (event.from === this.item.id) this.finish();
      return;
    }
    const item = this.itemFor(event.id);
    if (!item) return;
    const shown = item === this.item;
    if (event.t === "error") {
      item.error = { code: event.code };
      item.playing = false;
      if (shown) this.fire("error");
      return;
    }
    const hadDuration = Number.isFinite(item.duration) && item.duration > 0;
    const wasReady = item.ready;
    const wasPlaying = item.playing;
    if (event.dur > 0) item.duration = event.dur;
    if (!item.ended || event.ended) item.currentTime = event.pos;
    item.bufferedEnd = event.buf;
    item.ready = event.ready;
    item.playing = event.playing;
    // A play or pause from outside the page (audio focus, headphones out, the lock screen). Reports
    // older than the page's last command do not count.
    const external = event.seq >= this.seq && !event.ended;
    const pausedNow = external && item.paused !== !event.pwr;
    if (pausedNow) item.paused = !event.pwr;
    if (!shown) return;
    if (!hadDuration && Number.isFinite(item.duration) && item.duration > 0) {
      this.fire("loadedmetadata");
      this.fire("durationchange");
    }
    if (!wasReady && item.ready) this.fire("canplay");
    if (pausedNow) this.fire(item.paused ? "pause" : "play");
    if (!wasPlaying && item.playing) this.fire("playing");
    this.fire("timeupdate");
    this.fire("progress");
    if (event.ended && !item.ended) this.finish();
  }

  private itemFor(id: number): Item | null {
    if (id === this.item.id && id !== 0) return this.item;
    if (this.advanced && this.advanced.item.id === id) return this.advanced.item;
    return null;
  }

  private finish(): void {
    const item = this.item;
    if (item.ended) return;
    item.ended = true;
    item.playing = false;
    if (Number.isFinite(item.duration) && item.duration > 0) item.currentTime = item.duration;
    const wasPaused = item.paused;
    item.paused = true;
    if (!wasPaused) this.fire("pause");
    this.fire("ended");
  }
}

let instance: NativeAudio | null = null;

/** The one native player bridge, wired to the app's events. Android in the Tauri shell only. */
export function nativeAudio(send?: NativeSend): NativeAudio {
  if (instance) return instance;
  const audio = new NativeAudio(send ?? ((op, args) => invoke("android_audio", { args: { ...args, op } })));
  (window as unknown as { __tpAudio?: (event: NativeEvent) => void }).__tpAudio = (event) => audio.receive(event);
  instance = audio;
  return audio;
}
