import { lazy, Suspense, useEffect, useState } from "react";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useBridge, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import type { Issue } from "../../ipc/generated/Issue";
import type { Workspace } from "../../ipc/generated/Workspace";
import { projectsInOrder } from "../../state/selectors";
import { Button, Segmented } from "../../ui/controls";
import { agentsOn, agentToOpen, worktreesFor } from "./agents";
import { Detail } from "./Detail";
import { groupProjects, message, ticketId, ticketsFor } from "./data";
import { ticketChord, ticketKeyHint } from "./keys";
import { ProjectName, ProjectSwatch, TicketRow } from "./TicketRow";
import "./tickets.css";

const Assign = lazy(() => import("./Assign").then((m) => ({ default: m.Assign })));
const CloseTicket = lazy(() => import("./Cleanup").then((m) => ({ default: m.CloseTicket })));
const WHOSE = [
  { value: "mine", label: "Mine" },
  { value: "all", label: "All" },
] as const;
type Selection = { issue: Issue; projects: string[]; tracker: string; source: string | null };
type TicketDialog =
  | { kind: "assign"; selected: Selection; another: boolean }
  | { kind: "close"; selected: Selection; worktrees: Workspace[] };

export default function Tickets() {
  const bridge = useBridge();
  const app = useApp();
  const actions = useActions();
  const model = ticketsFor(bridge);
  const visible = useUi((s) => s.visible);
  const narrowed = useUi((s) => s.project);
  const chosen = useUi((s) => s.selected.tickets);
  const ids = useDaemon(useShallow((s) => projectsInOrder(s).map((p) => p.id)));
  const names = useDaemon(useShallow((s) => projectsInOrder(s).map((p) => p.name)));
  const groupName = (projects: readonly string[]) =>
    projects.map((id) => names[ids.indexOf(id)] ?? "?").join(", ");
  const keymap = useDaemon((s) => s.preferences?.keys);
  const lists = useStore(model.store, (s) => s.lists);
  const changing = useStore(model.store, (s) => s.changing);
  const [dialog, setDialog] = useState<TicketDialog | null>(null);
  const shownIds = narrowed ? ids.filter((id) => id === narrowed) : ids;
  const groups = groupProjects(shownIds, lists);
  const entries: Selection[] = groups.flatMap((projects) => {
    const list = lists[projects[0] ?? ""];
    return list?.state === "loaded"
      ? list.value.issues.map((issue) => ({
          issue,
          projects,
          tracker: list.value.tracker,
          source: list.value.source,
        }))
      : [];
  });
  const selected =
    entries.find((e) => ticketId(e.projects[0] ?? "", e.issue.key) === chosen) ??
    entries.find((e) => e.issue.key === chosen?.split(":")[1]) ??
    entries[0];
  const bindings = {
    another: keymap?.another_agent ?? ["a"],
    browser: keymap?.ticket_in_browser ?? ["b"],
    close: keymap?.close_ticket ?? ["x"],
    refresh: keymap?.refresh_tickets ?? ["r"],
  };
  const keys = {
    another: bindings.another.map(ticketKeyHint).join(" / "),
    browser: bindings.browser.map(ticketKeyHint).join(" / "),
    close: bindings.close.map(ticketKeyHint).join(" / "),
    refresh: bindings.refresh.map(ticketKeyHint).join(" / "),
  };
  const agentCount = useDaemon((s) =>
    selected
      ? agentsOn(
          Object.values(s.sessions),
          worktreesFor(Object.values(s.workspaces), selected.projects, selected.issue.key),
        ).length
      : 0,
  );
  useEffect(() => {
    model.prune(ids);
  }, [model, ids]);
  useEffect(() => {
    if (visible) model.forgetFailed();
  }, [model, visible]);
  useEffect(() => {
    if (visible) for (const id of shownIds) if (!lists[id]) void model.fetch(id);
  }, [model, visible, shownIds, lists]);
  useEffect(() => {
    if (selected && chosen !== ticketId(selected.projects[0] ?? "", selected.issue.key))
      actions.select("tickets", ticketId(selected.projects[0] ?? "", selected.issue.key));
  }, [selected, chosen, actions]);
  useEffect(() => {
    actions.setHints([
      ...(selected
        ? [
            { key: "⏎", does: agentCount ? "open its agent" : "assign an agent" },
            ...(agentCount ? [{ key: keys.another, does: "another agent" }] : []),
            ...(selected.issue.url ? [{ key: keys.browser, does: "browser" }] : []),
            { key: keys.close, does: "close ticket" },
          ]
        : []),
      { key: keys.refresh, does: "fetch again" },
    ]);
    return () => actions.setHints([]);
  }, [actions, selected, agentCount, keys.another, keys.browser, keys.close, keys.refresh]);
  useEffect(() => {
    actions.keysTo(dialog ? "dialog" : "list");
    return () => actions.keysTo("list");
  }, [actions, dialog]);
  const openSession = (id: string) => {
    const screen = app.daemon.store.getState().derived.needs_you.includes(id) ? "needs-you" : "all";
    actions.show(screen);
    actions.select(screen, id);
    actions.showTerminal(id);
    actions.keysTo("list");
  };
  const assign = (another: boolean) => {
    if (selected) setDialog({ kind: "assign", selected, another });
  };
  const open = () => {
    if (!selected) return;
    const state = app.daemon.store.getState();
    const agent = agentToOpen(
      agentsOn(
        Object.values(state.sessions),
        worktreesFor(Object.values(state.workspaces), selected.projects, selected.issue.key),
      ),
    );
    if (agent) openSession(agent.id);
    else assign(false);
  };
  const close = () => {
    if (selected)
      setDialog({
        kind: "close",
        selected,
        worktrees: worktreesFor(
          Object.values(app.daemon.store.getState().workspaces),
          selected.projects,
          selected.issue.key,
        ),
      });
  };
  const browser = () => {
    if (!selected) return;
    void bridge.openUrl(selected.issue.url).catch((error) => actions.tell(message(error), true));
  };
  const refresh = () => {
    if (visible) {
      model.refresh(shownIds);
      actions.tell("Fetching the tickets again…");
    }
  };
  const move = (by: number) => {
    const at = entries.indexOf(selected as Selection);
    const next = entries[Math.max(0, Math.min(entries.length - 1, at + by))];
    if (next) actions.select("tickets", ticketId(next.projects[0] ?? "", next.issue.key));
  };
  const handlers: Record<string, () => void> = {
    Enter: open,
    ArrowDown: () => move(1),
    ArrowUp: () => move(-1),
  };
  for (const key of keymap?.move_down ?? ["j"]) handlers[ticketChord(key)] = () => move(1);
  for (const key of keymap?.move_up ?? ["k"]) handlers[ticketChord(key)] = () => move(-1);
  for (const [action, binding] of Object.entries(bindings))
    for (const key of binding)
      handlers[ticketChord(key)] = { another: () => assign(true), browser, close, refresh }[
        action as keyof typeof bindings
      ];
  useKeys(handlers);
  return (
    <div className="tickets-screen" data-keys={dialog ? "dialog" : "list"}>
      <section className="tickets-list" aria-label="Tickets">
        <div className="mu-queue-head">
          <h1 className="mu-queue-title">Tickets</h1>
          <span className="mu-meta">Each project's open tickets, and the agents on them.</span>
        </div>
        <div className="mu-queue tickets-queue">
          {!shownIds.length ? (
            <p className="tickets-state mu-meta">
              No projects yet: add one, and its open tickets show here.
            </p>
          ) : (
            groups.map((projects) => {
              const project = projects[0] ?? "";
              const list = lists[project];
              return (
                <section key={project} aria-label={`${groupName(projects)}'s tickets`}>
                  <div className="mu-group">
                    <ProjectSwatch id={project} />
                    <span className="mu-group-name">
                      {projects.map((id, i) => (
                        <span key={id}>
                          {i ? <span className="mu-quiet">, </span> : null}
                          <ProjectName id={id} />
                        </span>
                      ))}
                    </span>
                    <span className="mu-group-source">
                      {list?.state === "loaded"
                        ? `${list.value.tracker}${list.value.mine ? ", only yours" : ""}`
                        : ""}
                    </span>
                    <span className="mu-spacer" />
                    {list?.state === "loaded" ? (
                      <Segmented
                        label={`Which of ${groupName(projects)}'s tickets`}
                        segments={WHOSE}
                        value={list.value.mine ? "mine" : "all"}
                        busy={changing[project]}
                        onChange={(whose) => {
                          if ((whose === "mine") !== list.value.mine)
                            void model
                              .mine(projects, whose === "mine")
                              .catch((error) => actions.tell(message(error), true));
                        }}
                      />
                    ) : null}
                  </div>
                  {!list || list.state === "loading" ? (
                    <p className="tickets-state mu-meta" role="status">
                      Fetching its tickets…
                    </p>
                  ) : list.state === "failed" ? (
                    <div className="tickets-state">
                      <p className="tickets-error" role="alert">
                        Its tickets could not be fetched: {list.error}
                      </p>
                      <Button onClick={refresh}>Try again ({keys.refresh})</Button>
                    </div>
                  ) : !list.value.issues.length ? (
                    <p className="tickets-state mu-meta">
                      {list.value.mine ? "none of yours open" : "no open tickets"}
                    </p>
                  ) : (
                    list.value.issues.map((issue) => (
                      <TicketRow
                        key={issue.key}
                        issue={issue}
                        projects={projects}
                        selected={
                          selected?.projects[0] === project && selected.issue.key === issue.key
                        }
                        onSelect={() => actions.select("tickets", ticketId(project, issue.key))}
                      />
                    ))
                  )}
                </section>
              );
            })
          )}
        </div>
      </section>
      {selected ? (
        <Detail
          issue={selected.issue}
          projects={selected.projects}
          tracker={selected.tracker}
          model={model}
          onAssign={() => assign(agentCount > 0)}
          onOpen={openSession}
          onClose={close}
          onBrowser={browser}
          keys={keys}
        />
      ) : (
        <section className="mu-pane tickets-detail">
          <div className="mu-empty">
            <p className="mu-empty-text">Select a ticket to see it here.</p>
          </div>
        </section>
      )}
      <Suspense fallback={null}>
        {dialog?.kind === "assign" ? (
          <Assign
            issue={dialog.selected.issue}
            projects={dialog.selected.projects}
            source={dialog.selected.source}
            another={dialog.another}
            model={model}
            onClose={() => setDialog(null)}
          />
        ) : dialog?.kind === "close" ? (
          <CloseTicket
            issue={dialog.selected.issue}
            projects={dialog.selected.projects}
            tracker={dialog.selected.tracker}
            worktrees={dialog.worktrees}
            model={model}
            onClose={() => setDialog(null)}
          />
        ) : null}
      </Suspense>
    </div>
  );
}
