import { describe, expect, it } from "vitest";
import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import { MockBridge } from "./bridge";
import recorded from "./replies.json";
import { workingDay } from "./scenario";

// The mock daemon tells the truth (ARC-24): the same steps the link's test
// sends the real daemon (src-tauri/tests/replies.rs), sent to the mock,
// get the same answers and the same refusals, word for word, with what
// the steps made named as the recording names it. The recording is the
// real daemon's; `pnpm replies` writes it afresh and CI fails when the
// daemon answers otherwise.

interface Sent {
  what: string;
  send: unknown;
  save: string | null;
  outcome: unknown;
}
interface Checkout {
  what: string;
  checkout: string;
  save: string;
}
interface Exited {
  what: string;
  exited: string;
}
type Step = Sent | Checkout | Exited;

/** A repository in the mock's file system, as the recording's `$repo`. */
const REPO = "/Users/you/code/replies/app";
const MISSING = "01JD0000000000000000000999";
/** Where the stand-in programs are, as the recording's `$bin`. */
const BIN = "/Users/you/.local/bin";

/** What the steps made, by name, and the values they stand for. */
class Names {
  private readonly named: [string, string][] = [];

  set(name: string, value: string): void {
    this.named.push([`$${name}`, value]);
    // Longest first, so `$repo/x` is filled in before a shorter name.
    this.named.sort(([, a], [, b]) => b.length - a.length);
  }

  get(name: string): string {
    return this.named.find(([key]) => key === name)?.[1] ?? name;
  }

  fill(text: string): string {
    return this.named.reduce((text, [name, value]) => text.replaceAll(name, value), text);
  }

  name(text: string): string {
    return this.named.reduce((text, [name, value]) => text.replaceAll(value, name), text);
  }
}

/** The id in a reply that made something (`ProjectAdded`, `WorktreeCreated`, `SessionStarted`). */
function made(reply: unknown): string {
  const [made] = Object.values(reply as Record<string, { id: string }>);
  return made?.id ?? "";
}

async function outcomeOf(mock: MockBridge, command: Command) {
  try {
    return { reply: await mock.command(command) };
  } catch (error) {
    if (!(error instanceof BridgeFailure)) throw error;
    return { refused: { code: error.code, message: error.message } };
  }
}

describe("ARC-24 the mock daemon answers as the real one does", () => {
  const mock = new MockBridge(workingDay());
  mock.flows.folders["/Users/you/code"]?.push({
    name: "replies",
    path: "/Users/you/code/replies",
    git: false,
  });
  mock.flows.folders["/Users/you/code/replies"] = [{ name: "app", path: REPO, git: true }];
  mock.flows.folders[REPO] = [];
  // As the link's test runs the daemon: no agents of the person's, and the
  // stand-in codex alone on its PATH, which says no version.
  mock.world.preferences.agents = [];
  for (const harness of Object.keys(mock.settings.installed))
    delete mock.settings.installed[harness];
  mock.settings.installed.codex = { path: `${BIN}/codex`, version: null };
  const names = new Names();
  names.set("repo", REPO);
  names.set("bin", BIN);
  names.set("missing", MISSING);

  for (const step of recorded as Step[]) {
    it(step.what, async () => {
      if ("checkout" in step) {
        const project = names.get(step.checkout);
        const checkout = mock.world.workspaces.find(
          (workspace) => workspace.project === project && workspace.kind.kind === "checkout",
        );
        expect(checkout).toBeDefined();
        names.set(step.save, checkout?.id ?? "");
        return;
      }
      if ("exited" in step) {
        const session = names.get(step.exited);
        // The daemon tells the program's exit after its answer.
        await Promise.resolve();
        await Promise.resolve();
        expect(mock.world.sessions.find((stored) => stored.id === session)?.status.state).toBe(
          "exited",
        );
        return;
      }
      const command = JSON.parse(names.fill(JSON.stringify(step.send))) as Command;
      const outcome = await outcomeOf(mock, command);
      if (step.save && "reply" in outcome) names.set(step.save, made(outcome.reply));
      expect(JSON.parse(names.name(JSON.stringify(outcome)))).toEqual(step.outcome);
    });
  }
});
