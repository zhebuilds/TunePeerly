// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef } from "react";

// Popovers that the Android back button closes before anything else, newest first.
const closers: (() => void)[] = [];

export function closeTopPopover(): boolean {
  const close = closers.pop();
  if (!close) return false;
  close();
  return true;
}

export function useBackClose(open: boolean, close: () => void) {
  const latest = useRef(close);
  latest.current = close;
  useEffect(() => {
    if (!open) return;
    const entry = () => latest.current();
    closers.push(entry);
    return () => {
      const at = closers.lastIndexOf(entry);
      if (at >= 0) closers.splice(at, 1);
    };
  }, [open]);
}
