import { lazy, Suspense, useEffect, useState } from "react";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useBridge, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import { Page } from "../../app/Page";
import { SidebarAwayRow } from "../../app/Sidebar";
import type { Issue } from "../../ipc/generated/Issue";
import type { Workspace } from "../../ipc/generated/Workspace";
import { labelOf } from "../../keys/keymap";
import { useActionKeys, useKeymap } from "../../keys/useKeymap";
import { projectsInOrder } from "../../state/selectors";
import { Button, Segmented, Spacer } from "../../ui/controls";
import { EmptyState, EmptyStateText } from "../../ui/EmptyState";
import { Heading, HeadingLead, HeadingTitle } from "../../ui/Heading";
import { List } from "../../ui/List";
import { Loading, LoadingBar } from "../../ui/Loading";
import { ErrorText } from "../../ui/status";
import { agentsOn, agentToOpen, worktreesFor } from "./agents";
import { Detail } from "./Detail";
import { groupProjects, message, ticketId, ticketsFor } from "./data";
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

/** A ticket's line's id in the window, for its list's `aria-activedescendant`. */
function lineIdOf(project: string, key: Issue["key"]): string {
  return `ticket-${ticketId(project, key)}`;
}

export default function Tickets() {
  const bridge = useBridge();
  const app = useApp();
  const actions = useActions();
  const model = ticketsFor(bridge);
  const visible = useUi((s) => s.visible);
  const chosen = useUi((s) => s.selected.tickets);
  const ids = useDaemon(useShallow((s) => projectsInOrder(s).map((p) => p.id)));
  const names = useDaemon(useShallow((s) => projectsInOrder(s).map((p) => p.name)));
  const groupName = (projects: readonly string[]) =>
    projects.map((id) => names[ids.indexOf(id)] ?? "?").join(", ");
  const map = useKeymap();
  const lists = useStore(model.store, (s) => s.lists);
  const changing = useStore(model.store, (s) => s.changing);
  const [dialog, setDialog] = useState<TicketDialog | null>(null);
  const groups = groupProjects(ids, lists);
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
  // The person's keys, as hints and tooltips show them.
  const keys = {
    another: labelOf(map, "another_agent"),
    browser: labelOf(map, "ticket_in_browser"),
    close: labelOf(map, "close_ticket"),
    refresh: labelOf(map, "refresh_tickets"),
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
    if (visible) for (const id of ids) if (!lists[id]) void model.fetch(id);
  }, [model, visible, ids, lists]);
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
      model.refresh(ids);
      actions.tell("Fetching the tickets again…");
    }
  };
  const move = (by: number) => {
    const at = entries.indexOf(selected as Selection);
    const next = entries[Math.max(0, Math.min(entries.length - 1, at + by))];
    if (next) actions.select("tickets", ticketId(next.projects[0] ?? "", next.issue.key));
  };
  useKeys({ Enter: open, ArrowDown: () => move(1), ArrowUp: () => move(-1) });
  useActionKeys("tickets", {
    move_down: () => move(1),
    move_up: () => move(-1),
    refresh_tickets: refresh,
    another_agent: () => assign(true),
    ticket_in_browser: browser,
    close_ticket: close,
  });
  // Each project's tickets are a list of their own: the keys moving the
  // selection into another project's list take the focus there with it.
  const selectedLine = selected ? lineIdOf(selected.projects[0] ?? "", selected.issue.key) : null;
  useEffect(() => {
    if (!selectedLine) return;
    const focused = document.activeElement;
    const into = document.getElementById(selectedLine)?.closest<HTMLElement>('[role="listbox"]');
    if (into && focused !== into && focused?.closest(".tickets-list")) {
      into.focus({ preventScroll: true });
    }
  }, [selectedLine]);
  return (
    <div className="tickets-screen" data-keys={dialog ? "dialog" : "list"}>
      <section className="tickets-list" aria-label="Tickets">
        <SidebarAwayRow />
        <div className="app-column-heading" data-tauri-drag-region>
          <Heading>
            <HeadingTitle level={1}>Tickets</HeadingTitle>
            <HeadingLead>Each project's open tickets, and the agents on them.</HeadingLead>
          </Heading>
        </div>
        <div className="tickets-queue">
          {!ids.length ? (
            <p className="tickets-state mu-meta">
              No projects yet: add one, and its open tickets show here.
            </p>
          ) : (
            groups.map((projects) => {
              const project = projects[0] ?? "";
              const list = lists[project];
              return (
                <section
                  key={project}
                  className="tickets-project"
                  aria-label={`${groupName(projects)}'s tickets`}
                >
                  {list?.state === "loaded" && list.refreshing ? (
                    <LoadingBar label={`Fetching ${groupName(projects)}'s tickets again`} />
                  ) : null}
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
                    <Spacer />
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
                    <Loading label="Fetching its tickets" layout="inline" />
                  ) : list.state === "failed" ? (
                    <div className="tickets-state">
                      <ErrorText>Its tickets could not be fetched: {list.error}</ErrorText>
                      <Button onClick={refresh}>Try again ({keys.refresh})</Button>
                    </div>
                  ) : !list.value.issues.length ? (
                    <p className="tickets-state mu-meta">
                      {list.value.mine ? "none of yours open" : "no open tickets"}
                    </p>
                  ) : (
                    <List
                      label={`${groupName(projects)}'s tickets`}
                      activeId={
                        selected?.projects[0] === project
                          ? lineIdOf(project, selected.issue.key)
                          : undefined
                      }
                      resting={dialog !== null}
                      className="tickets-group"
                    >
                      {list.value.issues.map((issue) => (
                        <TicketRow
                          key={issue.key}
                          lineId={lineIdOf(project, issue.key)}
                          issue={issue}
                          projects={projects}
                          selected={
                            selected?.projects[0] === project && selected.issue.key === issue.key
                          }
                          onSelect={() => actions.select("tickets", ticketId(project, issue.key))}
                        />
                      ))}
                    </List>
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
        <Page label="No ticket" className="tickets-detail">
          <EmptyState>
            <EmptyStateText>Select a ticket to see it here.</EmptyStateText>
          </EmptyState>
        </Page>
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
