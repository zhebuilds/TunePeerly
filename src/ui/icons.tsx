// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import type { ReactNode } from "react";

type IconProps = { className?: string };

function Line({ className, children }: IconProps & { children: ReactNode }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {children}
    </svg>
  );
}

function Solid({ className, children }: IconProps & { children: ReactNode }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

export const Icons = {
  home: (p: IconProps) => <Line {...p}><path d="M4 10.4 12 4l8 6.4V19a1 1 0 0 1-1 1h-4.5v-5.5h-5V20H5a1 1 0 0 1-1-1z" /></Line>,
  homeFill: (p: IconProps) => <Solid {...p}><path d="M12 3.2 3.4 10a1 1 0 0 0-.4.8V20a1 1 0 0 0 1 1h5.5v-6h5v6H20a1 1 0 0 0 1-1v-9.2a1 1 0 0 0-.4-.8z" /></Solid>,
  search: (p: IconProps) => <Line {...p}><circle cx="11" cy="11" r="6.5" /><path d="m20 20-4.2-4.2" /></Line>,
  library: (p: IconProps) => <Line {...p}><path d="M5 4v16M10 4v16" /><path d="m14.5 4.6 4.6 15" /></Line>,
  heart: (p: IconProps) => <Line {...p}><path d="M12 20s-7.5-4.6-7.5-10A4.2 4.2 0 0 1 12 7.6 4.2 4.2 0 0 1 19.5 10C19.5 15.4 12 20 12 20z" /></Line>,
  heartFill: (p: IconProps) => <Solid {...p}><path d="M12 20.6s-8.2-5-8.2-10.8a4.7 4.7 0 0 1 8.2-3.1 4.7 4.7 0 0 1 8.2 3.1c0 5.8-8.2 10.8-8.2 10.8z" /></Solid>,
  artist: (p: IconProps) => <Line {...p}><circle cx="12" cy="8.5" r="3.6" /><path d="M5 20a7 7 0 0 1 14 0" /></Line>,
  album: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="8.5" /><circle cx="12" cy="12" r="2.4" /><path d="M12 3.5a8.5 8.5 0 0 1 8.5 8.5" opacity=".45" /></Line>,
  folder: (p: IconProps) => <Line {...p}><path d="M3.5 7.5A2 2 0 0 1 5.5 5.5h3.6l2 2h7.4a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" /></Line>,
  devices: (p: IconProps) => <Line {...p}><rect x="3" y="5" width="13" height="9.5" rx="1.6" /><path d="M7 18.5h6" /><rect x="17.5" y="9" width="4" height="10" rx="1.2" /></Line>,
  settings: (p: IconProps) => (
    <Line {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </Line>
  ),
  plus: (p: IconProps) => <Line {...p}><path d="M12 5v14M5 12h14" /></Line>,
  play: (p: IconProps) => <Solid {...p}><path d="M8 5.2v13.6a.8.8 0 0 0 1.2.7l10.6-6.8a.8.8 0 0 0 0-1.4L9.2 4.5A.8.8 0 0 0 8 5.2z" /></Solid>,
  pause: (p: IconProps) => <Solid {...p}><rect x="6.5" y="5" width="3.8" height="14" rx="1" /><rect x="13.7" y="5" width="3.8" height="14" rx="1" /></Solid>,
  prev: (p: IconProps) => <Solid {...p}><rect x="5" y="5" width="2.4" height="14" rx="1" /><path d="M19 6.1v11.8a.8.8 0 0 1-1.2.7L9.4 12.7a.8.8 0 0 1 0-1.4l8.4-5.9a.8.8 0 0 1 1.2.7z" /></Solid>,
  next: (p: IconProps) => <Solid {...p}><rect x="16.6" y="5" width="2.4" height="14" rx="1" /><path d="M5 6.1v11.8a.8.8 0 0 0 1.2.7l8.4-5.9a.8.8 0 0 0 0-1.4L6.2 5.4A.8.8 0 0 0 5 6.1z" /></Solid>,
  shuffle: (p: IconProps) => <Line {...p}><path d="M3.5 7h3.2a4 4 0 0 1 3.3 1.8l4 6.4a4 4 0 0 0 3.3 1.8h3.2" /><path d="M3.5 17h3.2a4 4 0 0 0 3.3-1.8M14 8.8A4 4 0 0 1 17.3 7h3.2" /><path d="m18 4 2.6 3L18 10M18 14l2.6 3L18 20" /></Line>,
  repeat: (p: IconProps) => <Line {...p}><path d="M4 11V9.5A3.5 3.5 0 0 1 7.5 6H19" /><path d="m16 3 3 3-3 3" /><path d="M20 13v1.5a3.5 3.5 0 0 1-3.5 3.5H5" /><path d="m8 21-3-3 3-3" /></Line>,
  repeatOne: (p: IconProps) => <Line {...p}><path d="M4 11V9.5A3.5 3.5 0 0 1 7.5 6H19" /><path d="m16 3 3 3-3 3" /><path d="M20 13v1.5a3.5 3.5 0 0 1-3.5 3.5H5" /><path d="m8 21-3-3 3-3" /><path d="M11.2 10.4 12.6 9.5v5" /></Line>,
  queue: (p: IconProps) => <Line {...p}><path d="M4 6h12M4 11h12M4 16h7" /><path d="M15.5 14.2v6.3l5-3.15z" fill="currentColor" stroke="none" /></Line>,
  lyrics: (p: IconProps) => <Line {...p}><path d="M5 6.5h9M5 11h9M5 15.5h5" /><path d="M18.5 7.5v8.2" /><circle cx="16.8" cy="16" r="1.9" /><path d="m18.5 7.5 2.2-1" /></Line>,
  volume: (p: IconProps) => <Line {...p}><path d="M4 9.5v5h3.5l4.5 4V5.5l-4.5 4z" /><path d="M15.5 9a4.2 4.2 0 0 1 0 6M18.2 6.4a8 8 0 0 1 0 11.2" /></Line>,
  volumeLow: (p: IconProps) => <Line {...p}><path d="M4 9.5v5h3.5l4.5 4V5.5l-4.5 4z" /><path d="M15.5 9a4.2 4.2 0 0 1 0 6" /></Line>,
  volumeMute: (p: IconProps) => <Line {...p}><path d="M4 9.5v5h3.5l4.5 4V5.5l-4.5 4z" /><path d="m16 9.5 5 5M21 9.5l-5 5" /></Line>,
  close: (p: IconProps) => <Line {...p}><path d="M6.5 6.5l11 11M17.5 6.5l-11 11" /></Line>,
  minimize: (p: IconProps) => <Line {...p}><path d="M5 17h14" /></Line>,
  maximize: (p: IconProps) => <Line {...p}><rect x="5" y="5" width="14" height="14" rx="1.4" /></Line>,
  restore: (p: IconProps) => <Line {...p}><rect x="7.5" y="8.5" width="10" height="10" rx="1.2" /><path d="M9.5 8.5V6.4A1.4 1.4 0 0 1 10.9 5h6.7A1.4 1.4 0 0 1 19 6.4v6.7a1.4 1.4 0 0 1-1.4 1.4H16" /></Line>,
  back: (p: IconProps) => <Line {...p}><path d="m14.5 5.5-6.5 6.5 6.5 6.5" /></Line>,
  forward: (p: IconProps) => <Line {...p}><path d="m9.5 5.5 6.5 6.5-6.5 6.5" /></Line>,
  chevronDown: (p: IconProps) => <Line {...p}><path d="m6.5 9.5 5.5 5.5 5.5-5.5" /></Line>,
  chevronRight: (p: IconProps) => <Line {...p}><path d="m9.5 6.5 5.5 5.5-5.5 5.5" /></Line>,
  more: (p: IconProps) => <Solid {...p}><circle cx="5.5" cy="12" r="1.8" /><circle cx="12" cy="12" r="1.8" /><circle cx="18.5" cy="12" r="1.8" /></Solid>,
  check: (p: IconProps) => <Line {...p}><path d="m5 12.5 4.5 4.5L19 7.5" /></Line>,
  pencil: (p: IconProps) => <Line {...p}><path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z" /><path d="m13.5 6.5 4 4" /></Line>,
  trash: (p: IconProps) => <Line {...p}><path d="M4.5 7h15M10 11v6M14 11v6" /><path d="M6.5 7l1 12a2 2 0 0 0 2 1.8h5a2 2 0 0 0 2-1.8l1-12M9 7V4.8A.8.8 0 0 1 9.8 4h4.4a.8.8 0 0 1 .8.8V7" /></Line>,
  download: (p: IconProps) => <Line {...p}><path d="M12 4v11" /><path d="m7.5 10.5 4.5 4.5 4.5-4.5" /><path d="M5 19.5h14" /></Line>,
  eye: (p: IconProps) => <Line {...p}><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" /><circle cx="12" cy="12" r="3" /></Line>,
  eyeOff: (p: IconProps) => <Line {...p}><path d="M10.6 5.6A9 9 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-2.6 3.4M6.6 6.6C3.9 8.4 2.5 12 2.5 12S6 18.5 12 18.5a9 9 0 0 0 5.3-1.7" /><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2M3 3l18 18" /></Line>,
  warning: (p: IconProps) => <Line {...p}><path d="M10.3 4.3 2.8 17.5A2 2 0 0 0 4.5 20.5h15a2 2 0 0 0 1.7-3L13.7 4.3a2 2 0 0 0-3.4 0z" /><path d="M12 9.5v4M12 17h.01" /></Line>,
  info: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="9" /><path d="M12 11v5.5M12 7.8h.01" /></Line>,
  sun: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="4" /><path d="M12 2.5v2M12 19.5v2M4.6 4.6 6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4" /></Line>,
  moon: (p: IconProps) => <Line {...p}><path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z" /></Line>,
  incognito: (p: IconProps) => <Line {...p}><path d="M3 11h18M6 11l1.6-5.2a1 1 0 0 1 1.2-.7l3.2.9 3.2-.9a1 1 0 0 1 1.2.7L18 11" /><circle cx="7.5" cy="16" r="2.8" /><circle cx="16.5" cy="16" r="2.8" /><path d="M10.3 16a2.2 2.2 0 0 1 3.4 0" /></Line>,
  broadcast: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="2" /><path d="M16.2 7.8a6 6 0 0 1 0 8.4" /><path d="M7.8 16.2a6 6 0 0 1 0-8.4" /><path d="M19 5a10 10 0 0 1 0 14" /><path d="M5 19a10 10 0 0 1 0-14" /></Line>,
  sync: (p: IconProps) => <Line {...p}><path d="M20 11a8 8 0 0 0-14.3-4.3L4 9" /><path d="M4 4v5h5" /><path d="M4 13a8 8 0 0 0 14.3 4.3L20 15" /><path d="M20 20v-5h-5" /></Line>,
  copy: (p: IconProps) => <Line {...p}><rect x="8.5" y="8.5" width="11" height="11" rx="2" /><path d="M15.5 8.5V6.5a2 2 0 0 0-2-2h-7a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2" /></Line>,
  link: (p: IconProps) => <Line {...p}><path d="M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1" /><path d="M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1" /></Line>,
  wifi: (p: IconProps) => <Line {...p}><path d="M2.5 8.8a14 14 0 0 1 19 0M5.5 12.2a9.5 9.5 0 0 1 13 0M8.6 15.5a5 5 0 0 1 6.8 0" /><path d="M12 19h.01" /></Line>,
  globe: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></Line>,
  sidebar: (p: IconProps) => <Line {...p}><rect x="3.5" y="4.5" width="17" height="15" rx="2.5" /><path d="M9.5 4.5v15" /></Line>,
  music: (p: IconProps) => <Line {...p}><path d="M9 18V6.5l10-2V16" /><circle cx="6.8" cy="18" r="2.3" /><circle cx="16.8" cy="16" r="2.3" /></Line>,
  clock: (p: IconProps) => <Line {...p}><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></Line>,
  bell: (p: IconProps) => <Line {...p}><path d="M6 16.2h12l-1.3-2V10a4.7 4.7 0 0 0-9.4 0v4.2z" /><path d="M10 16.2a2 2 0 0 0 4 0" /></Line>,
  pin: (p: IconProps) => <Line {...p}><path d="M9 4.5h6l-1 5.2 2.2 2.3H7.8L10 9.7z" /><path d="M12 12v7.5" /></Line>,
  grip: (p: IconProps) => <Solid {...p}><circle cx="9" cy="6.5" r="1.4" /><circle cx="15" cy="6.5" r="1.4" /><circle cx="9" cy="12" r="1.4" /><circle cx="15" cy="12" r="1.4" /><circle cx="9" cy="17.5" r="1.4" /><circle cx="15" cy="17.5" r="1.4" /></Solid>,
  list: (p: IconProps) => <Line {...p}><path d="M9 6h11M9 12h11M9 18h11" /><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" /></Line>,
  sort: (p: IconProps) => <Line {...p}><path d="M7 4.5v15M3.8 16.5 7 19.7l3.2-3.2" /><path d="M13 7h7.5M13 12h5.5M13 17h3.5" /></Line>,
  filter: (p: IconProps) => <Line {...p}><circle cx="11" cy="11" r="6.5" /><path d="m20 20-4.2-4.2" /></Line>,
  external: (p: IconProps) => <Line {...p}><path d="M14 4.5h5.5V10M19.5 4.5 11 13" /><path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" /></Line>,
};

