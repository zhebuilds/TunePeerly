// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
type Clock = { position: number; duration: number };
type Transport = { playing: boolean };

/** The player engine writes this computer's playback here, and every progress bar and play button reads it. */
function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set: (next: T) => {
      value = next;
      listeners.forEach((listener) => listener());
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export const clock = createStore<Clock>({ position: 0, duration: 0 });
export const transport = createStore<Transport>({ playing: false });
