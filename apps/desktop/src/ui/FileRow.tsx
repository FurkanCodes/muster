import type { FileChange } from "../ipc/generated/FileChange";
import { splitPath } from "../lib/format";
import { DiffStat } from "./status";
import "./file-row.css";

// Git's letter for each kind, and the design system's colour for it.
const KIND = {
  modified: ["M", "M"],
  added: ["A", "A"],
  deleted: ["D", "D"],
  renamed: ["R", "R"],
  copied: ["C", "A"],
  type_changed: ["T", "R"],
  conflicted: ["U", "D"],
  untracked: ["?", "U"],
} as const;

/** Git's letter for a kind of change, and the `mu-file-kind--*` colour it takes. */
export function fileKind(kind: FileChange["kind"]): readonly [string, string] {
  return KIND[kind];
}

/** One changed file: its kind, its path (less `folder`, said once above the list), its lines. */
export function FileRow({
  file,
  folder = "",
  selected,
  onSelect,
  onOpen,
}: {
  file: FileChange;
  folder?: string;
  selected: boolean;
  onSelect(): void;
  onOpen(): void;
}) {
  const shown = file.path.startsWith(folder) ? file.path.slice(folder.length) : file.path;
  const path = splitPath(shown);
  const [letter, tone] = KIND[file.kind];
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      aria-label={`${file.kind}: ${file.from ? `${file.from} → ` : ""}${file.path}`}
      tabIndex={selected ? 0 : -1}
      title={file.path}
      className="mu-file"
      onFocus={onSelect}
      onClick={onOpen}
    >
      <span className={`mu-file-kind mu-file-kind--${tone}`} aria-hidden="true">
        {letter}
      </span>
      <span className="mu-file-path ui-file-path">
        <bdi>
          {file.from ? `${file.from} → ` : ""}
          <span className="mu-file-dir">{path.dir}</span>
          {path.name}
        </bdi>
      </span>
      {file.lines ? <DiffStat {...file.lines} /> : <span className="mu-quiet">binary</span>}
    </button>
  );
}
