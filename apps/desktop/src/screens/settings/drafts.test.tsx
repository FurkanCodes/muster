// A choice on a project's page that still needs a field is the person's
// until they finish it or leave it: the answer to a change sent before it
// never takes it away (ARC-24). The real daemon answers after the person
// may have moved on.

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { AppContext, type App as AppValue } from "../../app/context";
import { installKeys } from "../../app/keys";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { createDaemon } from "../../state/daemon";
import { createUi } from "../../state/ui";
import Screen from "./Settings";

function renderSettings() {
  const bridge = new MockBridge(workingDay());
  const { store, actions } = createUi();
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  actions.show("settings");
  void bridge.open((message) => app.daemon.apply(message));
  const keys = installKeys(app);
  render(
    <AppContext value={app}>
      <Screen />
    </AppContext>,
  );
  return { app, bridge, keys };
}

describe("ARC-24 a project's choice waiting for its field", () => {
  it("stays as the answer to a change sent before it comes back", async () => {
    const { app, bridge, keys } = renderSettings();
    act(() => app.actions.select("settings", `project:${ulid(1)}/only_mine`));
    const mine = await screen.findByRole("switch", { name: "Only my tickets" });
    bridge.hold("ChangeSetting");
    await userEvent.click(mine);
    await userEvent.click(screen.getByRole("button", { name: "Tickets from" }));
    await userEvent.click(await screen.findByRole("option", { name: "Linear" }));
    expect(await screen.findByLabelText(/^Team key, such as ENG/)).toBeInTheDocument();
    act(() => bridge.release("ChangeSetting"));
    await waitFor(() =>
      expect(app.ui.getState().note?.text).toBe(
        "Tickets for muster now lists only the tickets assigned to you.",
      ),
    );
    expect(screen.getByLabelText(/^Team key, such as ENG/)).toBeInTheDocument();
    keys();
  });
});

describe("ARC-24 a project's settings as the daemon answers them", () => {
  it("never go back to an answer older than one shown, nor say its words", async () => {
    const { app, bridge, keys } = renderSettings();
    act(() => app.actions.select("settings", `project:${ulid(1)}/only_mine`));
    const mine = await screen.findByRole("switch", { name: "Only my tickets" });
    // Made at once, answered late: the daemon answers each change from a task of its own.
    bridge.hold({ ChangeSetting: { project: ulid(1), change: { OnlyMyTickets: true } } }, "now");
    await userEvent.click(mine);
    await userEvent.click(screen.getByRole("button", { name: "Tickets from" }));
    await userEvent.click(await screen.findByRole("option", { name: "Linear" }));
    await userEvent.type(await screen.findByLabelText(/^Team key, such as ENG/), "ENG{Enter}");
    await waitFor(() =>
      expect(app.ui.getState().note?.text).toBe("muster's tickets now come from Linear, team ENG."),
    );
    // The answer to the first change, as things stood before Linear, comes last.
    act(() =>
      bridge.release({ ChangeSetting: { project: ulid(1), change: { OnlyMyTickets: true } } }),
    );
    await waitFor(() => expect(mine).not.toHaveAttribute("aria-busy"));
    expect(screen.getByRole("button", { name: "Tickets from" })).toHaveTextContent("Linear");
    expect(app.ui.getState().note?.text).toBe("muster's tickets now come from Linear, team ENG.");
    keys();
  });
});
