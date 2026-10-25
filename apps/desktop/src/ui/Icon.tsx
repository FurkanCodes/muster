// Line icons at 14 or 16 px, 1.75 wide, in currentColor (README: Iconography).
// Drawn inline: no icon font, no package, no network.

import { type PartProps, usePart } from "./part";

const PATHS = {
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm9 16-3.5-3.5",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6 6 18",
  // Lucide's diamond: a session placed where the preview is.
  place:
    "M2.7 10.3a2.41 2.41 0 0 0 0 3.41l7.59 7.59a2.41 2.41 0 0 0 3.41 0l7.59-7.59a2.41 2.41 0 0 0 0-3.41l-7.59-7.59a2.41 2.41 0 0 0-3.41 0Z",
  trash: "M4 7h16M10 7V4h4v3M6 7l1 13h10l1-13M10 11v6M14 11v6",
  expand: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  collapse: "M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5",
  external: "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
  chevron: "m6 9 6 6 6-6",
  undo: "M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3",
  sidebar: "M3.5 4.5h17v15h-17zM9 4.5v15",
  changes: "M12 4v8M8 8h8M8 18h8",
  terminal:
    "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM7 11l2-2-2-2M11 13h4",
  ticket:
    "M3 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2zM13 5v2M13 17v2M13 11v2",
  gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M5.6 18.4l1.8-1.8M16.6 7.4l1.8-1.8",
} as const;

export type IconName = keyof typeof PATHS | "more";

export function Icon({
  name,
  size = 14,
  ...props
}: PartProps<"svg"> & { name: IconName; size?: number }) {
  const shape =
    name === "more"
      ? {
          fill: "currentColor",
          children: (
            <>
              <circle cx="5" cy="12" r="1.6" />
              <circle cx="12" cy="12" r="1.6" />
              <circle cx="19" cy="12" r="1.6" />
            </>
          ),
        }
      : {
          fill: "none",
          stroke: "currentColor",
          strokeWidth: 1.75,
          strokeLinecap: "round" as const,
          strokeLinejoin: "round" as const,
          children: <path d={PATHS[name]} />,
        };
  return usePart("svg", props, {
    "aria-hidden": true,
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    ...shape,
  });
}
