# screens

## Overview

The desktop window's screens. Each folder holds one screen, built from the
components in `src/ui` and the `Page` and `Sidebar` of `src/app` (`ARC-26`).
The screen registry is `src/app/screens.tsx`. The session list and *Needs you*
are bundled eagerly; Tickets and Settings load on first show (`ARC-25`).

## Subdirectories & Boundaries

- queue: [`./queue/README.md`](./queue/README.md) — the *Needs you* list, in the model's `derived.needs_you` order, grouped by project, with the place-holding selection and the once-per-change flash.
- sessions: [`./sessions/README.md`](./sessions/README.md) — the All sessions tree in the sidebar (spaces, projects, workspaces, sessions; folds kept by the window) and the project page with its composer.
- settings: [`./settings/README.md`](./settings/README.md) — Settings as one flat list of rows across Muster's sections and each project's. Every change is a daemon `ChangePreference` or `ChangeSetting`.
- tickets: [`./tickets/README.md`](./tickets/README.md) — tickets per project group, their text and comments (data, never run), assigning an agent and closing a ticket.

## Shared rules

- Screens never sort or derive what the model already orders (`ARC-24`).
- A screen's keys are bound only while it is active, and its status-bar hints are cleared when it stops being active.
- Verification: `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/screens` (in `apps/desktop`).
