// Each panel of the session's pane (design system: Panel, as the AllSessions
// and PluginPanel boards draw them): the agent's, its logo, name and how it
// stands; the shell's, `❯ Shell` and the program in front in it; a plugin's,
// its plugin's name and its own title; and the line between two, which a drag or the
// arrows on it resize.

import { type CSSProperties, type KeyboardEvent, lazy, type PointerEvent, Suspense } from "react";
import { useApp, useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { panelName } from "../keys/keymap";
import { useKeyLabel } from "../keys/useKeymap";
import { agentOf } from "../lib/agents";
import { doing, statusKind } from "../lib/status";
import { branchOf } from "../state/selectors";
import { Panel, PanelClose, PanelHead, PanelTitle } from "../ui/Panel";
import { AgentLogo, BranchName, StatusMark } from "../ui/status";
import { putPanelAway, putShellAway } from "./beside";
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

export function SessionPanel({
  session,
  style,
  lifted,
  onHeadDown,
}: BoxProps & { session: SessionId }) {
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const kind = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored ? statusKind(stored) : "fresh";
  });
  const words = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored ? doing(stored) : "";
  });
  const name = agentOf(harness).name;
  return (
    <Panel
      label={name}
      hasKeys={useHasKeys(session)}
      className={`panels-box ${lifted ? "is-lifted" : ""}`}
      style={style}
    >
      <PanelHead onPointerDown={onHeadDown} title="Drag to move it">
        <AgentLogo harness={harness} size="sm" />
        <PanelTitle>{name}</PanelTitle>
        <StatusMark kind={kind} />
        <span className="mu-cut">{words}</span>
      </PanelHead>
      <Body session={session} compact={false} />
    </Panel>
  );
}

export function ShellPanel({
  shell,
  shown,
  style,
  lifted,
  onHeadDown,
}: BoxProps & { shell: SessionId; shown: SessionId }) {
  const app = useApp();
  const program = useDaemon((state) => state.sessions[shell]?.title ?? "");
  // A shell of another workspace says whose it is.
  const elsewhere = useDaemon((state) => {
    const own = state.sessions[shell]?.workspace;
    const workspace =
      own && own !== state.sessions[shown]?.workspace ? state.workspaces[own] : null;
    return workspace ? branchOf(workspace) : null;
  });
  const key = useKeyLabel("shell");
  return (
    <Panel
      label="Shell"
      hasKeys={useHasKeys(shell)}
      className={`panels-box ${lifted ? "is-lifted" : ""}`}
      style={style}
    >
      <PanelHead onPointerDown={onHeadDown} title="Drag to move it">
        <span className="mu-mono panels-prompt" aria-hidden="true">
          ❯
        </span>
        <PanelTitle>Shell</PanelTitle>
        <span className="mu-cut">{program}</span>
        {elsewhere ? <BranchName className="mu-cut">{elsewhere}</BranchName> : null}
        <PanelClose
          label="Put the shell away"
          keyHint={key}
          onClose={() => putShellAway(app, shown)}
        />
      </PanelHead>
      <Body session={shell} compact />
    </Panel>
  );
}

export function PluginPanel({
  panel,
  shown,
  style,
  lifted,
  onHeadDown,
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
      <PanelHead onPointerDown={onHeadDown} title="Drag to move it">
        <span className="mu-mono" aria-hidden="true">
          ◇
        </span>
        <PanelTitle>{plugin}</PanelTitle>
        <span className="mu-cut">{title}, from the plugin</span>
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
  session: "the agent",
  changes: "the changes",
  shell: "the shell",
  plugin: "the plugin's panel",
} as const;

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
      aria-label={`Between ${divider.first.map((one) => NAMES[one]).join(" and ")} and ${divider.second.map((one) => NAMES[one]).join(" and ")}`}
      title="Drag to share the place between them"
      className={`panels-line panels-line--${divider.split} ${dragged ? "is-dragged" : ""}`}
      style={style}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}
