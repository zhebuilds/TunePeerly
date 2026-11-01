// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { invoke } from "@tauri-apps/api/core";

/** What the phone's player reports (`NativeAudio.kt`). */
export type NativeEvent =
  | { t: "state"; id: number; seq: number; pos: number; dur: number; buf: number; pwr: boolean; playing: boolean; ready: boolean; ended: boolean }
  | { t: "error"; id: number; code: number; msg?: string }
  | QueueEvent
  | { t: "heard"; id: number; key: string }
  | { t: "restarted"; id: number };

/** The phone's player moved the queue on by itself (a song ended, next or previous on the lock screen), or was asked where it is. */
export type QueueEvent = {
  t: "queue";
  rev: number;
  seq: number;
  reason: string;
  id: number;
  past: string[];
  current: string | null;
  future: string[];
  repeat: string;
  order: string;
  contextId: string;
  playing: boolean;
  ended: boolean;
  pos: number;
};

export type NativeSend = (op: string, args: Record<string, unknown>) => Promise<unknown>;

/** A song the phone's player may start by itself. `u` is set for songs on the phone; others stream by content id `c`. */
export type CatalogSong = { k: string; u: string | null; c: string | null; t: string; a: string; l: string; p: string | null; f: boolean; d: number };

/** The queue and its rules, so the phone's player can go on without the page (see `advance.ts`). */
export type QueuePlan = {
  past: string[];
  current: string | null;
  future: string[];
  repeat: string;
  order: string;
  contextId: string;
  pool: string[];
  library: string[];
};

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
 * The phone's player also holds the whole queue (`setCatalog`, `setPlan`) and moves on by itself when
 * a song ends or next or previous is pressed on the lock screen, so a held-back script never stops the
 * music. Each move comes back as a queue event; the page then takes the new song over (`claim`).
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
  /** The newest change the phone's player made to the queue that the page has seen. */
  private rev = 0;
  /** The command number of the page's last load; queue events from before it are out of date. */
  private loadSeq = 0;
  private plan: QueuePlan | null = null;
  private planSent = "";
  private librarySent = "";
  private poolSent = "";
  private catalogSent = "";
  /** The start of the app's stream URLs on 127.0.0.1; add a content id. Set once known. */
  streamBase = "";
  /** Why the last song failed, as far as the page needs to know. */
  errorReason: "missing" | "other" = "other";
  /** The key of the song the next `load` is for. */
  loadKey: string | null = null;
  onQueue: ((event: QueueEvent) => void) | null = null;
  onHeard: ((key: string) => void) | null = null;

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
    this.send("load", { id: this.item.id, url: this.src_, key: this.loadKey ?? "" });
    this.loadSeq = this.seq;
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

  /** Every song the phone's player may start by itself, and the start of the app's stream URLs. */
  setCatalog(base: string, songs: CatalogSong[]): void {
    const sig = base + "\n" + JSON.stringify(songs);
    if (sig === this.catalogSent) return;
    this.catalogSent = sig;
    this.send("catalog", { base, songs });
  }

  /** The queue and its rules. The pool and library are sent again only when they changed. */
  setPlan(plan: QueuePlan): void {
    this.plan = plan;
    this.sendPlan(false);
  }

  private sendPlan(force: boolean): void {
    const plan = this.plan;
    if (!plan) return;
    const pool = plan.pool.join("\n");
    const library = plan.library.join("\n");
    const body: Record<string, unknown> = {
      rev: this.rev,
      past: plan.past,
      current: plan.current ?? "",
      future: plan.future,
      repeat: plan.repeat,
      order: plan.order,
      contextId: plan.contextId,
    };
    if (force || pool !== this.poolSent) body.pool = plan.pool;
    if (force || library !== this.librarySent) body.library = plan.library;
    const sig = JSON.stringify(body);
    if (!force && sig === this.planSent) return;
    this.planSent = sig;
    this.poolSent = pool;
    this.librarySent = library;
    this.send("queue", body);
  }

  /** Asks the phone's player where it is; it answers with a queue event. */
  sync(): void {
    this.send("sync");
  }

  /** Takes over the song the phone's player started by itself. Call `replay` once listeners are bound. */
  claim(id: number, url: string): void {
    if (this.item.id === id) return;
    const item = fresh(id);
    item.paused = false;
    this.item = item;
    this.src_ = url;
  }

  get currentId(): number {
    return this.item.id;
  }

  /** Sends the events a fresh listener missed for the claimed song. */
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
    if (event.t === "queue") {
      if (event.rev > this.rev) this.rev = event.rev;
      if (event.seq < this.loadSeq) {
        // The page loaded another song after this move; the phone's player plays that one now. Send
        // the page's queue again so both agree.
        this.sendPlan(true);
        return;
      }
      this.onQueue?.(event);
      return;
    }
    if (event.t === "heard") {
      this.onHeard?.(event.key);
      return;
    }
    if (event.t === "restarted") {
      if (event.id === this.item.id) {
        this.item.ended = false;
        this.item.currentTime = 0;
        this.fire("timeupdate");
      }
      return;
    }
    const item = this.itemFor(event.id);
    if (!item) return;
    const shown = item === this.item;
    if (event.t === "error") {
      item.error = { code: event.code };
      this.errorReason = /FILE_NOT_FOUND|NO_PERMISSION/.test(event.msg ?? "") ? "missing" : "other";
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
