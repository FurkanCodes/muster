// Each panel of the session's pane (design system: Panel, as the AllSessions
// and PluginPanel boards draw them): the preview's, the agent's logo, name
// and how it stands, and among others `preview`, a `◆` placing its session
// where it is and a `✕` folding it away; a terminal placed in a panel of its
// own, an agent's the same with its title, a shell's `❯` and the program in
// front in it, each saying where it works and with a `✕` taking it off the
// screen, its session running on; a plugin's, its plugin's name and its own
// title; and the line between two, which a drag or the arrows on it resize.
// A double-click on a head zooms its panel over the others, or puts it back,
// and a zoomed head says how many it hides.

import { type CSSProperties, type KeyboardEvent, lazy, type PointerEvent, Suspense } from "react";
import { useApp, useDaemon, useUi } from "../app/context";
import { IconButton } from "../ui/controls";
import type { SessionId } from "../ipc/generated/SessionId";
import { panelName } from "../keys/keymap";
import { useKeyLabel } from "../keys/useKeymap";
import { agentOf } from "../lib/agents";
import { doing, statusKind } from "../lib/status";
import { branchOf, shownHarness } from "../state/selectors";
import { Panel, PanelClose, PanelHead, PanelTitle } from "../ui/Panel";
import { AgentLogo, BranchName, StatusMark } from "../ui/status";
import { putPanelAway } from "./beside";
import { type PanelName, placedSession } from "./layout";
import type { Divider } from "./solve";

// xterm is the heaviest part of the window: loaded once a terminal shows (ARC-25).
const Terminal = lazy(() =>
  import("../terminal/Terminal").then((module) => ({ default: module.Terminal })),
);

interface BoxProps {
  style: CSSProperties;
  /** Being carried by its head. */
  lifted: boolean;
  onHeadDown: (event: PointerEvent<HTMLDivElement>) => void;
  /** A double-click on its head: zoomed over the others, or put back. */
  onZoom: () => void;
  /** How many others it shows over, zoomed; null while it is not. */
  hiding: number | null;
}

/** What a zoomed panel's head says: how many it shows over. */
function Zoomed({ hiding }: { hiding: number | null }) {
  if (hiding === null) return null;
  return <span className="mu-cut panels-zoomed">zoomed · {hiding} more</span>;
}

/** Where `session` works, for a head among others': its project and its branch. */
function useWhere(session: SessionId): string {
  return useDaemon((state) => {
    const workspace = state.workspaces[state.sessions[session]?.workspace ?? ""];
    const project = workspace ? state.projects[workspace.project] : undefined;
    if (!workspace || !project) return "";
    const branch = branchOf(workspace);
    return branch ? `${project.name}/${branch}` : project.name;
  });
}

function useHasKeys(session: SessionId): boolean {
  return useUi((state) => state.keysIn === "terminal" && state.terminal === session);
}

function Body({ session, compact }: { session: SessionId; compact: boolean }) {
  return (
    <div className="panels-body">
      <Suspense fallback={null}>
        <Terminal session={session} compact={compact} />
      </Suspense>
    </div>
  );
}

/** The agent's terminal alone, framed as the Terminal component is. */
export function TerminalAlone({ session }: { session: SessionId }) {
  return (
    <div className="panels-alone">
      <Suspense fallback={null}>
        <Terminal session={session} />
      </Suspense>
    </div>
  );
}

/** The agent's logo, name, how it stands and what it is doing, as its panel's head says them. */
function useAgentHead(session: SessionId) {
  const harness = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored ? shownHarness(stored) : "";
  });
  const kind = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored ? statusKind(stored) : "fresh";
  });
  const words = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored ? doing(stored) : "";
  });
  const title = useDaemon((state) => state.sessions[session]?.title ?? "");
  return { harness, kind, words, title, name: agentOf(harness).name };
}

/**
 * A terminal placed in a panel of its own: an agent's title and how it
 * stands, or a shell's prompt and the program in front in it; where it
 * works; and a way to take it off the screen, running on.
 */
export function PlacedPanel({
  session,
  style,
  lifted,
  onHeadDown,
  onZoom,
  hiding,
  onTakeOff,
}: BoxProps & { session: SessionId; onTakeOff: () => void }) {
  const { harness, kind, words, title } = useAgentHead(session);
  const shell = harness === "shell";
  const where = useWhere(session);
  const key = useKeyLabel("pin_session");
  return (
    <Panel
      label={shell ? `Shell ${title}` : title}
      hasKeys={useHasKeys(session)}
      className={`panels-box ${lifted ? "is-lifted" : ""}`}
      style={style}
    >
      <PanelHead onPointerDown={onHeadDown} onDoubleClick={onZoom} title="Drag to move it">
        {shell ? (
          <span className="mu-mono panels-prompt" aria-hidden="true">
            ❯
          </span>
        ) : (
          <AgentLogo harness={harness} size="sm" />
        )}
        <PanelTitle>{shell ? "Shell" : title}</PanelTitle>
        {shell ? null : <StatusMark kind={kind} />}
        <span className="mu-cut">{shell ? title : words}</span>
        {where ? <BranchName className="mu-cut">{where}</BranchName> : null}
        <Zoomed hiding={hiding} />
        <PanelClose
          label={`Take ${shell ? "the shell" : title} off the screen`}
          keyHint={key}
          onClose={onTakeOff}
        />
      </PanelHead>
      <Body session={session} compact={shell} />
    </Panel>
  );
}

/**
 * The preview: the session the list selects. Among others it says so, and
 * offers to place its session where it is (`◆`) or to fold it away until
 * another is selected (`✕`).
 */
export function SessionPanel({
  session,
  style,
  lifted,
  onHeadDown,
  onZoom,
  hiding,
  among,
}: BoxProps & {
  session: SessionId;
  /** Beside others: its place and its folding, else none. */
  among: { onPlace: () => void; onFold: () => void } | null;
}) {
  const { harness, kind, words, name } = useAgentHead(session);
  const where = useWhere(session);
  const key = useKeyLabel("pin_session");
  return (
    <Panel
      label={among ? `Preview: ${name}` : name}
      hasKeys={useHasKeys(session)}
      className={`panels-box ${lifted ? "is-lifted" : ""}`}
      style={style}
    >
      <PanelHead onPointerDown={onHeadDown} onDoubleClick={onZoom} title="Drag to move it">
        {among ? <span className="mu-cut panels-preview">preview ·</span> : null}
        <AgentLogo harness={harness} size="sm" />
        <PanelTitle>{name}</PanelTitle>
        <StatusMark kind={kind} />
        <span className="mu-cut">{words}</span>
        {among && where ? <BranchName className="mu-cut">{where}</BranchName> : null}
        <Zoomed hiding={hiding} />
        {among ? (
          <>
            <IconButton
              icon="place"
              size="sm"
              label="Place it here"
              title={`Place it here, in a panel of its own (${key})`}
              onClick={among.onPlace}
              onPointerDown={(event) => event.stopPropagation()}
            />
            <PanelClose label="Fold the preview away" keyHint={null} onClose={among.onFold} />
          </>
        ) : null}
      </PanelHead>
      <Body session={session} compact={false} />
    </Panel>
  );
}

export function PluginPanel({
  panel,
  shown,
  style,
  lifted,
  onHeadDown,
  onZoom,
  hiding,
}: BoxProps & { panel: SessionId; shown: SessionId }) {
  const app = useApp();
  const title = useDaemon((state) => state.sessions[panel]?.title ?? "");
  const harness = useDaemon((state) => state.sessions[panel]?.harness ?? "");
  const [pluginId = "", panelId = ""] = harness.replace(/^plugin:/, "").split("/");
  const plugin = useDaemon(
    (state) => state.plugins.find((one) => one.id === pluginId)?.details?.name ?? pluginId,
  );
  const key = useKeyLabel(panelName(pluginId, panelId));
  return (
    <Panel
      label={plugin}
      hasKeys={useHasKeys(panel)}
      className={`panels-box ${lifted ? "is-lifted" : ""}`}
      style={style}
    >
      <PanelHead onPointerDown={onHeadDown} onDoubleClick={onZoom} title="Drag to move it">
        <span className="mu-mono" aria-hidden="true">
          ◇
        </span>
        <PanelTitle>{plugin}</PanelTitle>
        <span className="mu-cut">{title}, from the plugin</span>
        <Zoomed hiding={hiding} />
        <PanelClose
          label={`Put the ${title} away`}
          keyHint={key === "none" ? null : key}
          onClose={() => void putPanelAway(app, panel, title, shown)}
        />
      </PanelHead>
      <Body session={panel} compact />
    </Panel>
  );
}

const NAMES = {
  list: "the list",
  session: "the preview",
  changes: "the changes",
  plugin: "the plugin's panel",
} as const;

/** `panel` as a screen reader says it. */
function nameOf(panel: PanelName): string {
  return placedSession(panel) === null ? NAMES[panel as keyof typeof NAMES] : "a terminal placed";
}

/** The step the arrows move a line by, in percent. */
const STEP = 5;

export function PanelLine({
  divider,
  share,
  dragged,
  style,
  onPointerDown,
  onShare,
}: {
  divider: Divider;
  share: number;
  dragged: boolean;
  style: CSSProperties;
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onShare: (share: number) => void;
}) {
  const across = divider.split === "across";
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const less = across ? "ArrowLeft" : "ArrowUp";
    const more = across ? "ArrowRight" : "ArrowDown";
    if (event.key !== less && event.key !== more) return;
    // The arrows are the line's here, not the list's.
    event.preventDefault();
    event.stopPropagation();
    const next = Math.min(90, Math.max(10, share + (event.key === more ? STEP : -STEP)));
    if (next !== share) onShare(next);
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a line dragged between two panels, not a rule between sections.
    <div
      role="separator"
      tabIndex={0}
      aria-orientation={across ? "vertical" : "horizontal"}
      aria-valuenow={share}
      aria-valuemin={10}
      aria-valuemax={90}
      aria-label={`Between ${divider.first.map(nameOf).join(" and ")} and ${divider.second.map(nameOf).join(" and ")}`}
      title="Drag to share the place between them"
      className={`panels-line panels-line--${divider.split} ${dragged ? "is-dragged" : ""}`}
      style={style}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}
