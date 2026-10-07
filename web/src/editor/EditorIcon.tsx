const ICON_PATHS = {
  "arrow-left": ["M19 12H5", "M12 19l-7-7 7-7"],
  "arrow-right": ["M5 12h14", "M12 5l7 7-7 7"],
  folder: ["M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"],
  search: ["M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z", "m21 21-4.3-4.3"],
  close: ["M18 6 6 18", "m6 6 12 12"],
  error: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 8v4", "M12 16h.01"],
  warning: ["m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z", "M12 9v4", "M12 17h.01"],
  ok: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "m9 12 2 2 4-4"],
  "off-scene": ["M9.88 9.88a3 3 0 1 0 4.24 4.24", "M10.73 5.08A10.4 10.4 0 0 1 12 5c7 0 10 7 10 7a13.2 13.2 0 0 1-1.67 2.68", "M6.61 6.61A13.5 13.5 0 0 0 2 12s3 7 10 7a9.7 9.7 0 0 0 5.39-1.61", "m2 2 20 20"],
  image: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z", "M9 11a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z", "m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"],
  frame: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z"],
  pointer: ["M4 4l7.07 17 2.51-7.39L21 11.07Z"],
  "chevron-down": ["m6 9 6 6 6-6"],
  "chevron-up": ["m18 15-6-6-6 6"],
  copy: ["M20 9H11a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2v-9a2 2 0 0 0-2-2Z", "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"],
  trash: ["M3 6h18", "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", "M10 11v6", "M14 11v6"],
  undo: ["M9 14 4 9l5-5", "M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5v0a5.5 5.5 0 0 1-5.5 5.5H11"],
  plus: ["M12 5v14", "M5 12h14"],
  play: ["M6 3v18l15-9Z"],
  pause: ["M7 4h3v16H7z", "M14 4h3v16h-3z"],
  stop: ["M5 5h14v14H5z"],
  "step-forward": ["M5 4l10 8-10 8V4Z", "M19 5v14"],
  "step-back": ["M19 4 9 12l10 8V4Z", "M5 5v14"],
  "volume-on": ["M11 5 6 9H2v6h4l5 4V5Z", "M15.5 8.5a5 5 0 0 1 0 7", "M19 5a10 10 0 0 1 0 14"],
  "volume-off": ["M11 5 6 9H2v6h4l5 4V5Z", "M22 9l-6 6", "M16 9l6 6"],
  replay: ["M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", "M3 3v5h5"],
  move: ["M5 9l-3 3 3 3", "M9 5l3-3 3 3", "M15 19l-3 3-3-3", "M19 9l3 3-3 3", "M2 12h20", "M12 2v20"],
  rotate: ["M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8", "M21 3v5h-5"],
  scale: ["M21 3 9 15", "M12 3H3v18h18v-9", "M16 3h5v5", "M14 15H9v-5"],
  "terrain-raise": ["M3 20c2.5-5 5.5-7.5 9-7.5s6.5 2.5 9 7.5", "M12 9V3", "m9 6 3-3 3 3"],
  "terrain-level": ["M3 12h18", "M7 3v5", "m5 6 2 2 2-2", "M17 21v-5", "m15 18 2-2 2 2"],
  "terrain-imprint": ["M2 12c1.5-4.5 3-7 5-7s3.5 2.5 5 7", "M12 12c1.5 4.5 3 7 5 7s3.5-2.5 5-7"],
  materials: ["m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z", "m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65", "m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"],
  "terrain-smooth": ["m3 9 3-4 3 4 3-4 3 4 3-4 3 4", "M3 19c3-3 6-3 9 0s6 3 9 0"],
  water: ["M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7Z"],
  wind: ["M17.7 7.7a2.5 2.5 0 1 1 1.8 4.3H2", "M9.6 4.6A2 2 0 1 1 11 8H2", "M12.6 19.4A2 2 0 1 0 14 16H2"],
  more: ["M12 6a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z", "M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z", "M12 20a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z"],
  download: ["M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "M7 10l5 5 5-5", "M12 15V3"],
} as const;

export type EditorIconName = keyof typeof ICON_PATHS;

type EditorIconProps = { name: EditorIconName; size?: number; className?: string };

/** Линейные значки редактора в сетке 24×24 — цвет берут из `currentColor`. */
export function EditorIcon({ name, size = 16, className }: EditorIconProps): React.JSX.Element {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {ICON_PATHS[name].map((path) => (
        <path key={path} d={path} />
      ))}
    </svg>
  );
}
