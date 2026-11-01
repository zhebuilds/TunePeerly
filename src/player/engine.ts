// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { api, errorText, type Track } from "../api";
import { trackKey } from "../lib/library";
import { clock, transport } from "./clock";
import type { NativeAudio, Upcoming } from "./native";

export type FailureKind = "transport" | "decode" | "unsupported" | "missing" | "offline";
export type Failure = { reason: string; kind: FailureKind };

export type EngineEvents = {
  ended: () => void;
  failed: (track: Track, failure: Failure) => void;
  heard: (track: Track) => void;
  paused: (track: Track, position: number) => void;
  seeked: (track: Track, position: number, paused: boolean) => void;
  progress: (position: number) => void;
  activity: (playing: boolean, track: Track | null, bufferEnough: boolean) => void;
  playable: (track: Track) => void;
};

type Source = { url: string; remote: boolean; deviceId: string | null };

/** Owns the one HTMLAudioElement. Every load gets a generation; listeners from older loads are removed before the next one starts. */
export class PlaybackEngine {
  readonly audio: HTMLAudioElement;
  private generation = 0;
  private listeners: AbortController | null = null;
  private track: Track | null = null;
  private source: Source | null = null;
  private pendingSeek = 0;
  private failedDevices: string[] = [];
  private skipLocal = false;
  private reloads = 0;
  private heardSeconds = 0;
  private heardSent = false;
  private lastTime = 0;
  private lastProgress = 0;
  private bufferEnough = false;
  private reportedPlayable = false;
  private wantPlay = false;
  private playPending = false;
  private playTicket = 0;
  private endedSent = false;
  private replayGuard = false;
  private upTicket = 0;
  /** Android's player in the app (see native.ts). Null on desktop, which plays in the web view. */
  private readonly native: NativeAudio | null;

  constructor(private readonly events: EngineEvents, native: NativeAudio | null = null) {
    this.native = native;
    this.audio = native ? (native as unknown as HTMLAudioElement) : new Audio();
    this.audio.preload = "auto";
  }

  /**
   * Android only: tells the phone's player which song follows, so it can start it by itself when this
   * one ends, even while the page's script is held back with the screen off. Desktop ignores it.
   */
  async lineUp(track: Track | null, meta: Upcoming["meta"] | null): Promise<void> {
    const native = this.native;
    if (!native) return;
    const ticket = ++this.upTicket;
    if (!track || !meta || track.video || (this.track && sameMedia(this.track, track))) {
      native.setUpcoming(null);
      return;
    }
    const key = trackKey(track);
    if (native.upcomingKey === key) return;
    try {
      const local = track.absolutePath && track.accessStatus === "available" && track.badge !== "remote";
      let source: Source;
      if (local && track.absolutePath) source = { url: api.fileUrl(track.absolutePath), remote: false, deviceId: null };
      else if (track.contentId) {
        const remote = await api.remoteUrl(track.contentId, []);
        source = { url: remote.url, remote: true, deviceId: remote.deviceId };
      } else {
        native.setUpcoming(null);
        return;
      }
      if (ticket !== this.upTicket) return;
      native.setUpcoming({ key, url: source.url, remote: source.remote, deviceId: source.deviceId, meta });
    } catch {
      if (ticket === this.upTicket) native.setUpcoming(null);
    }
  }

  get current(): Track | null {
    return this.track;
  }

  get position(): number {
    return this.source ? this.audio.currentTime : this.pendingSeek;
  }

  get playing(): boolean {
    return Boolean(this.source) && !this.audio.paused;
  }

  get hasSource(): boolean {
    return this.source !== null;
  }

  get remoteDevice(): string | null {
    return this.source?.remote ? this.source.deviceId : null;
  }

  /** Shows a track at a position without loading bytes. Used for the restored queue, which stays paused. */
  cue(track: Track | null, position: number): void {
    this.generation += 1;
    this.detach();
    this.track = track;
    this.pendingSeek = Math.max(0, position);
    this.failedDevices = [];
    this.skipLocal = false;
    clock.set({ position: this.pendingSeek, duration: (track?.durationMs ?? 0) / 1000 });
    transport.set({ playing: false });
  }

  /** A new track, or the same track starting over. */
  start(track: Track, position = 0, autoplay = true): Promise<void> {
    const replay = Boolean(this.source && !this.source.remote && this.track && sameMedia(this.track, track) && position <= 0.05);
    this.generation += 1;
    const generation = this.generation;
    this.track = track;
    this.failedDevices = [];
    this.skipLocal = false;
    this.reloads = 0;
    this.heardSeconds = 0;
    this.heardSent = false;
    this.reportedPlayable = false;
    this.endedSent = false;
    if (replay && this.source) {
      this.replayGuard = true;
      try {
        this.audio.currentTime = 0;
      } catch {
        // The element can refuse a seek until the next load; attach() covers that.
        this.replayGuard = false;
        return this.attach(generation, position, autoplay);
      }
      this.pendingSeek = 0;
      this.lastTime = 0;
      clock.set({ position: 0, duration: this.durationOf(track) });
      this.bind(generation, 0, autoplay, track, this.source);
      if (autoplay) this.resume();
      return Promise.resolve();
    }
    this.replayGuard = false;
    return this.attach(generation, position, autoplay);
  }

  toggle(): void {
    if (!this.track) return;
    if (!this.source) {
      void this.start(this.track, this.pendingSeek, true);
      return;
    }
    if (this.audio.paused) this.resume();
    else this.pause();
  }

  resume(): void {
    if (!this.track) return;
    if (!this.source) {
      void this.start(this.track, this.pendingSeek, true);
      return;
    }
    this.wantPlay = true;
    if (this.playPending) return;
    this.playPending = true;
    const generation = this.generation;
    const ticket = ++this.playTicket;
    this.audio.play().then(
      () => {
        if (ticket === this.playTicket) this.playPending = false;
      },
      (error: unknown) => {
        if (ticket === this.playTicket) this.playPending = false;
        if (generation !== this.generation) return;
        const name = error instanceof DOMException ? error.name : "";
        if (name === "AbortError") return;
        transport.set({ playing: false });
      },
    );
  }

  pause(): void {
    this.wantPlay = false;
    if (this.source) this.audio.pause();
  }

  seek(seconds: number): void {
    if (!this.track) return;
    const duration = this.source && Number.isFinite(this.audio.duration) ? this.audio.duration : (this.track.durationMs ?? 0) / 1000;
    const target = Math.max(0, Math.min(seconds, duration > 0 ? duration - 0.05 : seconds));
    if (!this.source) {
      this.pendingSeek = target;
      clock.set({ position: target, duration });
      this.events.seeked(this.track, target, true);
      return;
    }
    this.audio.currentTime = target;
    this.lastTime = target;
    clock.set({ position: target, duration });
    this.events.seeked(this.track, target, this.audio.paused);
  }

  skip(delta: number): void {
    this.seek(this.position + delta);
  }

  /** Unloads the current bytes and keeps the track shown at its position. */
  halt(): void {
    const position = this.position;
    this.generation += 1;
    this.detach();
    this.pendingSeek = position;
    transport.set({ playing: false });
    this.events.activity(false, this.track, false);
  }

  setVolume(volume: number, muted: boolean): void {
    this.audio.volume = Math.max(0, Math.min(1, volume));
    this.audio.muted = muted;
  }

  private detach(): void {
    this.listeners?.abort();
    this.listeners = null;
    this.audio.pause();
    if (this.audio.hasAttribute("src")) {
      this.audio.removeAttribute("src");
      this.audio.load();
    }
    this.source = null;
    this.bufferEnough = false;
    this.playTicket += 1;
    this.playPending = false;
  }

  private async resolve(track: Track): Promise<Source> {
    if (track.video) throw new Error("playback.video");
    const local = !this.skipLocal && track.absolutePath && track.accessStatus === "available" && track.badge !== "remote";
    if (local && track.absolutePath) return { url: api.fileUrl(track.absolutePath), remote: false, deviceId: null };
    if (track.contentId) {
      const remote = await api.remoteUrl(track.contentId, this.failedDevices);
      return { url: remote.url, remote: true, deviceId: remote.deviceId };
    }
    throw new Error(track.reasonKey ?? "notice.offlineTrack");
  }

  private async attach(generation: number, position: number, autoplay: boolean): Promise<void> {
    const track = this.track;
    if (!track) return;
    // Android: the phone's player may already be playing this song, started by itself when the last one ended.
    const adopted = position <= 0.05 ? this.native?.adopt(trackKey(track)) : null;
    if (adopted) {
      this.listeners?.abort();
      this.listeners = null;
      this.playTicket += 1;
      this.playPending = false;
      this.bufferEnough = false;
      this.pendingSeek = 0;
      this.lastTime = 0;
      this.source = adopted;
      clock.set({ position: this.audio.currentTime, duration: this.durationOf(track) });
      this.bind(generation, 0, autoplay, track, adopted);
      this.native?.replay();
      if (!autoplay) this.audio.pause();
      return;
    }
    this.detach();
    this.pendingSeek = position;
    this.lastTime = position;
    clock.set({ position, duration: (track.durationMs ?? 0) / 1000 });
    transport.set({ playing: false });
    let source: Source;
    try {
      source = await this.resolve(track);
    } catch (error) {
      if (generation !== this.generation) return;
      const reason = errorText(error);
      this.events.failed(track, { reason, kind: failureKind(reason) });
      return;
    }
    if (generation !== this.generation) return;
    this.source = source;
    this.bind(generation, position, autoplay, track, source);
    this.audio.src = source.url;
    this.audio.load();
  }

  /** Listens to the element that is already showing `source`. Replaces any previous listeners. */
  private bind(generation: number, position: number, autoplay: boolean, track: Track, source: Source): void {
    this.listeners?.abort();
    this.playTicket += 1;
    this.playPending = false;
    this.wantPlay = autoplay;
    const controller = new AbortController();
    this.listeners = controller;
    const on = (name: keyof HTMLMediaElementEventMap, handler: () => void) =>
      this.audio.addEventListener(name, handler, { signal: controller.signal });

    on("loadedmetadata", () => {
      if (position > 0 && (!Number.isFinite(this.audio.duration) || position < this.audio.duration)) this.audio.currentTime = position;
      clock.set({ position: this.audio.currentTime, duration: this.durationOf(track) });
      if (this.wantPlay) this.resume();
    });
    on("durationchange", () => clock.set({ position: this.audio.currentTime, duration: this.durationOf(track) }));
    on("timeupdate", () => {
      this.releaseReplayGuard();
      this.tick(track);
      this.maybeFinish(generation, track);
    });
    on("progress", () => this.checkBuffer(track));
    on("playing", () => {
      this.releaseReplayGuard();
      transport.set({ playing: true });
      this.events.activity(true, track, this.bufferEnough);
    });
    on("pause", () => {
      transport.set({ playing: false });
      this.events.activity(false, track, this.bufferEnough);
      if (!this.audio.ended) this.events.paused(track, this.audio.currentTime);
      this.maybeFinish(generation, track);
    });
    on("canplay", () => {
      this.reloads = 0;
      if (this.wantPlay && this.audio.paused) this.resume();
      if (!this.reportedPlayable && !source.remote) {
        this.reportedPlayable = true;
        this.events.playable(track);
      }
    });
    on("ended", () => this.markEnded(generation, track));
    on("error", () => void this.handleError(generation, track, source));
  }

  private releaseReplayGuard(): void {
    if (!this.replayGuard) return;
    const duration = this.audio.duration;
    const time = this.audio.currentTime;
    // A stale position near the end must not count as having left the end.
    const nearEnd = Number.isFinite(duration) && duration > 1 && time >= duration - 0.5;
    if (time > 0.3 && !nearEnd) this.replayGuard = false;
  }

  private markEnded(generation: number, track: Track): void {
    if (this.endedSent || this.replayGuard || generation !== this.generation) return;
    this.endedSent = true;
    transport.set({ playing: false });
    this.countHeard(track, true);
    this.events.ended();
  }

  private maybeFinish(generation: number, track: Track): void {
    if (!this.wantPlay || this.replayGuard) return;
    // The phone's player reports its own end and may already have started the next song; wait for it.
    if (this.native) return;
    const duration = this.audio.duration;
    if (!Number.isFinite(duration) || duration < 0.3) return;
    if (this.audio.ended || this.audio.currentTime >= duration - 0.12) this.markEnded(generation, track);
  }

  private durationOf(track: Track): number {
    return Number.isFinite(this.audio.duration) && this.audio.duration > 0 ? this.audio.duration : (track.durationMs ?? 0) / 1000;
  }

  private tick(track: Track): void {
    const now = this.audio.currentTime;
    clock.set({ position: now, duration: this.durationOf(track) });
    if (!this.audio.paused) {
      const delta = now - this.lastTime;
      if (delta > 0 && delta < 1.5) this.heardSeconds += delta;
      this.countHeard(track, false);
      const wall = performance.now();
      if (wall - this.lastProgress > 5000) {
        this.lastProgress = wall;
        this.events.progress(now);
      }
    }
    this.lastTime = now;
  }

  private countHeard(track: Track, ended: boolean): void {
    if (this.heardSent) return;
    const duration = this.durationOf(track);
    const needed = duration > 0 ? Math.min(10, duration * 0.5) : 10;
    if (this.heardSeconds >= needed || (ended && this.heardSeconds > 1)) {
      this.heardSent = true;
      this.events.heard(track);
    }
  }

  private checkBuffer(track: Track): void {
    const now = this.audio.currentTime;
    const duration = this.durationOf(track);
    let enough = false;
    for (let index = 0; index < this.audio.buffered.length; index += 1) {
      const start = this.audio.buffered.start(index);
      const end = this.audio.buffered.end(index);
      if (start <= now + 0.5 && end >= now) {
        enough = end - now >= 20 || (duration > 0 && end >= duration - 0.5);
        break;
      }
    }
    if (enough !== this.bufferEnough) {
      this.bufferEnough = enough;
      this.events.activity(!this.audio.paused, track, enough);
    }
  }

  private async handleError(generation: number, track: Track, source: Source): Promise<void> {
    if (generation !== this.generation) return;
    const code = this.audio.error?.code ?? 0;
    if (code === MediaError.MEDIA_ERR_ABORTED) return;
    const position = this.audio.currentTime || this.pendingSeek;
    const wasPlaying = this.wantPlay;
    const status = await probe(source.url);
    if (generation !== this.generation) return;
    const delivered = status === 200 || status === 206;
    const formatError = code === MediaError.MEDIA_ERR_DECODE || code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED;

    if (!source.remote) {
      const missing = status === 404 || status === 403;
      if (missing && track.contentId && track.holderIds.length > 0) {
        this.skipLocal = true;
        this.generation += 1;
        await this.attach(this.generation, position, wasPlaying);
        return;
      }
      if (status === 404) return this.fail(track, { reason: "file.missing", kind: "missing" });
      // Status 0 means the probe could not see the file URL at all, so nothing is known about these bytes yet.
      // Try another device first. If it delivers the bytes and they fail to decode, the remote branch below stops there.
      if (status === 0 && track.contentId && track.holderIds.length > 0) {
        this.skipLocal = true;
        this.generation += 1;
        await this.attach(this.generation, position, wasPlaying);
        return;
      }
      // A decoder rejection with the file readable is about these bytes.
      if (formatError) return this.fail(track, formatFailure(code));
      if (missing || status === 0 || code === MediaError.MEDIA_ERR_NETWORK) return this.fail(track, { reason: "file.unreadable", kind: "missing" });
      return this.fail(track, { reason: "playback.unsupported", kind: "unsupported" });
    }

    // The same content id is the same bytes. Once they actually arrived, another device will not decode them either.
    if (formatError && delivered) {
      const failure = formatFailure(code);
      if (track.contentId && source.deviceId) void api.remoteFailure(track.contentId, source.deviceId, failure.kind, failure.reason).catch(() => {});
      return this.fail(track, failure);
    }
    if (delivered && code === MediaError.MEDIA_ERR_NETWORK && this.reloads < 2) {
      // The route on 127.0.0.1 stays the same; the backend continues on whichever session reaches that device now.
      this.reloads += 1;
      await this.attach(generation, position, wasPlaying);
      return;
    }
    if (source.deviceId) this.failedDevices.push(source.deviceId);
    this.generation += 1;
    await this.attach(this.generation, position, wasPlaying);
  }

  private fail(track: Track, failure: Failure): void {
    const position = this.audio.currentTime || this.pendingSeek;
    this.detach();
    this.pendingSeek = position;
    transport.set({ playing: false });
    this.events.failed(track, failure);
  }
}

function sameMedia(left: Track, right: Track): boolean {
  if (left.contentId && right.contentId) return left.contentId === right.contentId;
  return Boolean(left.localFileId) && left.localFileId === right.localFileId;
}

function formatFailure(code: number): Failure {
  const kind = code === MediaError.MEDIA_ERR_DECODE ? "decode" : "unsupported";
  const reason = kind === "decode" ? "playback.decodeFailed" : "playback.unsupported";
  return { reason, kind };
}

function failureKind(reason: string): FailureKind {
  if (reason === "playback.decodeFailed") return "decode";
  if (reason === "playback.unsupported" || reason === "playback.video") return "unsupported";
  if (reason === "playback.allSourcesFailed") return "transport";
  if (reason.startsWith("playback.")) return "unsupported";
  return "offline";
}

async function probe(url: string): Promise<number> {
  try {
    const response = await fetch(url, { headers: { Range: "bytes=0-1" }, cache: "no-store" });
    void response.body?.cancel().catch(() => {});
    return response.status;
  } catch {
    return 0;
  }
}
