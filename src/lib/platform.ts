// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

/** The Android app: songs come in through the system picker and downloads stay in the app's own folder. */
export const isAndroid = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
