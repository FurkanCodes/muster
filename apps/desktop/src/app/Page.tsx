// What fills the window beside the sidebar (design system: PaneHeader): a
// pane with one row at its top, clear of the window's own buttons and with
// the way back to the sidebar while it is away, its title, what it is about
// and its actions at the end; then what it shows.
//
//   <Page label={title}>
//     <PageHeader>
//       <AgentLogo harness={harness} />
//       <PageTitle>{title}</PageTitle>
//       <PageMeta>…</PageMeta>
//       <PageActions>…</PageActions>
//     </PageHeader>
//     …
//   </Page>

import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { SidebarAway } from "./Sidebar";

export function Page({
  label,
  className,
  children,
}: {
  /** Its name, for a screen reader: what it shows. */
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={cn("mu-pane app-pane", className)} aria-label={label}>
      {children}
    </section>
  );
}

/** The row at its top: the window is dragged by it, and the sidebar is brought back from it. */
export function PageHeader({ children }: { children?: ReactNode }) {
  return (
    <header className="app-head" data-tauri-drag-region>
      <SidebarAway />
      {children}
    </header>
  );
}

export function PageTitle({
  title,
  className,
  children,
}: {
  /** All of it, for the tooltip, where the title is cut. */
  title?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <h2 className={cn("app-head-title", className)} title={title}>
      {children}
    </h2>
  );
}

/** What it is about, after its title: where it runs, what its work comes to. Cut first as room runs out. */
export function PageMeta({
  title,
  className,
  children,
}: {
  title?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span className={cn("mu-pane-meta app-head-where", className)} title={title}>
      {children}
    </span>
  );
}

/** Its actions, at the row's end; the room before them still drags the window. */
export function PageActions({ children }: { children: ReactNode }) {
  return (
    <>
      <span className="mu-spacer" data-tauri-drag-region />
      <div className="mu-pane-actions">{children}</div>
    </>
  );
}

/** The pane's place while nothing is selected in the list: the window keeps its shape. */
export function NothingSelected({ className }: { className?: string }) {
  return (
    <Page label="No session" className={className}>
      <PageHeader />
      <p className="mu-meta app-page-none">The session you select shows here, live.</p>
    </Page>
  );
}
