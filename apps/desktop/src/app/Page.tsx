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
import { Spacer } from "../ui/controls";
import { type PartProps, usePart } from "../ui/part";
import { SidebarAway } from "./Sidebar";

export function Page({
  label,
  ...props
}: PartProps<"section"> & {
  /** Its name, for a screen reader: what it shows. */
  label: string;
}) {
  return usePart("section", props, { className: "mu-pane app-pane", "aria-label": label });
}

/** The row at its top: the window is dragged by it, and the sidebar is brought back from it. */
export function PageHeader({ children, ...props }: PartProps<"header">) {
  return usePart("header", props, {
    className: "app-head",
    // The window is dragged by it (Tauri's own attribute).
    ...{ "data-tauri-drag-region": true },
    children: (
      <>
        <SidebarAway />
        {children}
      </>
    ),
  });
}

/** Its title; `title` all of it, for the tooltip, where the title is cut. */
export function PageTitle(props: PartProps<"h2">) {
  return usePart("h2", props, { className: "app-head-title" });
}

/** What it is about, after its title: where it runs, what its work comes to. Cut first as room runs out. */
export function PageMeta(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-pane-meta app-head-where" });
}

/** Its actions, at the row's end; the room before them still drags the window. */
export function PageActions({ children }: { children: ReactNode }) {
  return (
    <>
      <Spacer dragsWindow />
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
