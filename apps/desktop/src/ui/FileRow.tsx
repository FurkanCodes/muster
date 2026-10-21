import type { FileChange } from "../ipc/generated/FileChange";
import { splitPath } from "../lib/format";
import { DiffStat } from "./status";

const KIND = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  copied: "C",
  type_changed: "T",
  conflicted: "U",
  untracked: "?",
} as const;

export function FileRow({
  file,
  selected,
  onSelect,
  onOpen,
}: {
  file: FileChange;
  selected: boolean;
  onSelect(): void;
  onOpen(): void;
}) {
  const path = splitPath(file.path);
  const letter = KIND[file.kind];
  const tone =
    file.kind === "untracked"
      ? "U"
      : file.kind === "conflicted"
        ? "D"
        : file.kind === "copied"
          ? "A"
          : file.kind === "type_changed"
            ? "R"
            : letter;
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      aria-label={`${file.kind}: ${file.from ? `${file.from} → ` : ""}${file.path}`}
      tabIndex={selected ? 0 : -1}
      title={`${file.kind}: ${file.path}`}
      className="mu-file"
      onFocus={onSelect}
      onClick={onOpen}
    >
      <span className={`mu-file-kind mu-file-kind--${tone}`} aria-hidden="true">
        {letter}
      </span>
      <span className="mu-file-path">
        {file.from ? `${file.from} → ` : ""}
        <span className="mu-file-dir">{path.dir}</span>
        {path.name}
      </span>
      {file.lines ? (
        <DiffStat {...file.lines} />
      ) : (
        <span className="mu-meta">binary or too large to count</span>
      )}
    </button>
  );
}
