// The changes view end to end in WebKit, over the mock daemon: each state
// the Changes and Put back boards show, and the keys (ARC-24, ARC-25, PRD-14).

import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

declare global {
  interface Window {
    __muster: MockBridge;
  }
}
const workspace = "01JD0000000000000000000011";
const other = "01JD0000000000000000000012";

/** Opens the changes of Sketch the Tauri shell, the first session needing you. */
async function open(page: Page, before?: (page: Page) => Promise<void>) {
  await page.goto("/");
  await before?.(page);
  await page.getByRole("button", { name: "Changes", exact: true }).click();
  await expect(page.getByRole("button", { name: /^Not committed/ })).toBeVisible();
}

/**
 * The agent in the workspace finishes its turn, not yet seen, so nothing
 * works there and its session stays first under Needs you.
 */
async function agentDone(page: Page) {
  await page.evaluate((workspace) => {
    for (const session of window.__muster.world.sessions) {
      if (session.workspace === workspace) {
        session.status = { state: "finished" };
        session.unseen = true;
      } else if (session.status.state === "needs_you") session.status = { state: "running" };
    }
  }, workspace);
}

function sent(page: Page, name: string) {
  return page.evaluate(
    (name) =>
      window.__muster.sent.filter((command) => typeof command !== "string" && name in command),
    name,
  );
}

const statusBar = (page: Page) => page.locator(".mu-statusbar");

test("ARC-24 Not committed: files with kind, path and lines; ⏎ opens the diff; ⎋ back to the list, then the terminal", async ({
  page,
}) => {
  await open(page);
  await expect(page.getByRole("button", { name: "Changes", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  const files = page.getByRole("listbox", { name: "Files not committed" });
  await expect(files.getByRole("option")).toHaveText([
    "Msrc/link.rs+118−4",
    "?src/main.rs+61",
    "?Cargo.toml+24",
  ]);
  await expect(page.locator(".changes-toolbar .mu-diff")).toHaveText("+203−4in 3 files");
  await expect(page.getByText("Choose a file to see its diff.")).toBeVisible();
  await expect(statusBar(page)).toContainText("ask git again");
  await expect(statusBar(page)).toContainText("back to the terminal");

  await page.keyboard.press("Enter");
  const diff = page.getByRole("region", { name: "Diff" });
  await expect(diff).toBeFocused();
  await expect(diff.locator(".mu-dl--add").first()).toHaveText(
    "2+use muster_client::{Connection, Received};",
  );
  await expect(diff.locator(".mu-dl--del").first()).toHaveText("2−use muster_client::Connection;");
  await expect(page.locator(".mu-diffview-head")).toHaveText("Msrc/link.rs+118−4Not committed");
  await expect(statusBar(page)).toContainText("next and last hunk");

  await page.keyboard.press("Escape");
  await expect(files.getByRole("option", { name: "modified: src/link.rs" })).toBeFocused();
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => sent(page, "FileDiff"))
    .toEqual([
      { FileDiff: { workspace, part: "uncommitted", path: "src/link.rs" } },
      { FileDiff: { workspace, part: "uncommitted", path: "src/main.rs" } },
    ]);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Terminal", { exact: true })).toBeVisible();
});

test("ARC-24 On the branch: its commits since the base, then its files; ⇥ goes round the views", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "On the branch 2" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByRole("heading", { name: "Commits since main · 1" })).toBeVisible();
  await expect(page.locator(".changes-commits")).toHaveText("a1b2c3d Add the desktop crate");
  await expect(
    page.getByRole("heading", { name: "Changed on the branch · 2 files" }),
  ).toBeVisible();
  await page.getByRole("option", { name: "modified: Cargo.toml" }).click();
  await expect(page.locator(".mu-diffview-head")).toHaveText("MCargo.toml+12−2On the branch");
  await expect
    .poll(() => sent(page, "FileDiff"))
    .toEqual([{ FileDiff: { workspace, part: "branch", path: "Cargo.toml" } }]);
  await page.keyboard.press("Shift+Tab");
  await expect(page.getByRole("button", { name: "Not committed 3" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("ARC-24 Turns: each entry with its agent, what it changed and when; ⏎ asks its diff", async ({
  page,
}) => {
  await open(page);
  await page.getByRole("button", { name: "Turns 4" }).click();
  await expect(page.getByRole("listbox", { name: "Turns" }).getByRole("option")).toHaveText([
    "4Codex, turn 33 files +203−41m",
    "3✎Edits between turns1 file +25m",
    "2Codex, turn 22 files +40−425m",
    "1Codex, turn 11 file +12−440m",
  ]);
  await expect(page.getByText("Muster keeps each workspace's newest 100.")).toBeVisible();
  await expect(statusBar(page)).toContainText("put back");
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(page.locator(".mu-diffview-head")).toHaveText("Edits between turns+2");
  // The queue's This turn asked turn 4's diff for its files; the view asks entry 3's.
  await expect
    .poll(() => sent(page, "EntryDiff"))
    .toContainEqual({ EntryDiff: { workspace, number: 3 } });
});

test("PRD-14 put back: the plan, then the dialog as the board draws it, then Undo with the branch on b", async ({
  page,
}) => {
  await open(page, agentDone);
  await page.getByRole("button", { name: "Turns 4" }).click();
  await page.keyboard.press("j");
  await page.keyboard.press("j");
  await page.keyboard.press("z");
  const dialog = page.getByRole("dialog", { name: "Put the files back as before Codex's turn 2?" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("The worktree's files go back as they were 28m ago.");
  await expect(dialog.locator(".mu-details")).toHaveText(
    "Goes with itEdits between turns, 5m agoCodex, turn 3, 1m agoThe branchPut back with the branch also moves ⎇ feature/desktop-app back to 1234567, before its 1 commit since.CodexStill remembers that turn: tell it what you put back.",
  );
  await expect(dialog.getByRole("button")).toHaveText([
    "Keep it",
    "Put back the files",
    "Put back with the branch",
  ]);
  // ⏎ would put back the files only: moving the branch is chosen, on b (PRD-14).
  await expect(dialog.getByRole("button", { name: "Put back the files" })).toBeFocused();
  await page.keyboard.press("b");
  await expect(dialog).toBeHidden();
  await expect
    .poll(() => sent(page, "Undo"))
    .toEqual([{ Undo: { workspace, number: 2, move_branch: true } }]);
  await expect(statusBar(page)).toContainText(
    "The files are as they were before #2, and the branch with them. z on #5 brings them back.",
  );
  await expect(
    page.getByRole("listbox", { name: "Turns" }).getByRole("option", { selected: true }),
  ).toHaveText(/^5↺Put back as before #2/);
  await expect(
    page.getByRole("listbox", { name: "Turns" }).getByRole("option", { selected: true }),
  ).toBeFocused();
});

test("PRD-14 ⏎ puts back the files only, even where the branch could move too", async ({
  page,
}) => {
  await open(page, agentDone);
  await page.getByRole("button", { name: "Turns 4" }).click();
  await page.keyboard.press("j");
  await page.keyboard.press("j");
  await page.keyboard.press("z");
  const dialog = page.getByRole("dialog", { name: "Put the files back as before Codex's turn 2?" });
  await expect(dialog.getByRole("button", { name: "Put back with the branch" })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(dialog).toBeHidden();
  await expect
    .poll(() => sent(page, "Undo"))
    .toEqual([{ Undo: { workspace, number: 2, move_branch: false } }]);
  await expect(statusBar(page)).toContainText(
    "The files are as they were before #2. z on #5 brings them back.",
  );
});

test("PRD-14 put back files only with y; n keeps it, sending nothing", async ({ page }) => {
  await open(page, agentDone);
  await page.getByRole("button", { name: "Turns 4" }).click();
  await page.keyboard.press("z");
  const dialog = page.getByRole("dialog", { name: "Put the files back as before Codex's turn 3?" });
  await expect(dialog.getByRole("button")).toHaveText(["Keep it", "Put back the files"]);
  await page.keyboard.press("n");
  await expect(dialog).toBeHidden();
  expect(await sent(page, "Undo")).toEqual([]);
  await page.getByRole("button", { name: "Put back", exact: true }).click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press("y");
  await expect
    .poll(() => sent(page, "Undo"))
    .toEqual([{ Undo: { workspace, number: 4, move_branch: false } }]);
  await expect(statusBar(page)).toContainText(
    "The files are as they were before #4. z on #5 brings them back.",
  );
});

test("PRD-14 the TUI's refusals, said in the status bar: an agent at work, files changed since, no such entry", async ({
  page,
}) => {
  // Sketch the Tauri shell still asks its question: its agent is at work there.
  await open(page);
  await page.getByRole("button", { name: "Turns 4" }).click();
  await page.keyboard.press("z");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("y");
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(statusBar(page).locator(".mu-note--error")).toHaveText(
    "Sketch the Tauri shell is at work there: put the files back once it has stopped",
  );

  await agentDone(page);
  await page.evaluate(() => {
    window.__muster.failNext(
      "Undo",
      "the files were not put back: error: Entry 'src/link.rs' not uptodate. Cannot merge.",
      "conflict",
    );
  });
  await page.keyboard.press("z");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("y");
  await expect(statusBar(page).locator(".mu-note--error")).toHaveText(
    "the files were not put back: error: Entry 'src/link.rs' not uptodate. Cannot merge.",
  );

  await page.evaluate(() => {
    window.__muster.failNext(
      "UndoPlan",
      "there is no entry 4 in this workspace's timeline",
      "not_found",
    );
  });
  await page.keyboard.press("z");
  await expect(statusBar(page).locator(".mu-note--error")).toHaveText(
    "there is no entry 4 in this workspace's timeline",
  );
  await expect(page.getByRole("dialog")).toBeHidden();
  // z away from Turns says what to do.
  await page.keyboard.press("Tab");
  await page.keyboard.press("z");
  await expect(statusBar(page).locator(".mu-note--error")).toHaveText(
    "Choose a turn under Turns to put the files back as they were before it.",
  );
});

for (const [command, asking, lead, empty] of [
  ["WorkspaceChanges", "Asking git…", "", "Nothing here is waiting to be committed."],
  [
    "Timeline",
    "Reading the turns…",
    "The timeline could not be read: ",
    "None kept yet: each agent's turn here is, from now on.",
  ],
] as const) {
  test(`ARC-24 ${command}: asking, refused, asked again, empty`, async ({ page }) => {
    await open(page, async (page) => {
      // This turn, under the terminal, has its timeline before the view asks for its own.
      await expect(page.locator(".mu-turn")).toContainText("Codex, turn 3");
      await page.evaluate((command) => {
        window.__muster.hold(command);
      }, command);
    });
    if (command === "Timeline") await page.getByRole("button", { name: /^Turns/ }).click();
    await expect(page.getByText(asking)).toBeVisible();
    await page.evaluate((command) => {
      window.__muster.failNext(command, "git could not read the worktree", "internal");
      window.__muster.release(command);
    }, command);
    await expect(page.getByRole("alert")).toHaveText(
      `${lead}git could not read the worktreeAsk again`,
    );
    await page.evaluate(
      ({ workspace }) => {
        window.__muster.world.changes ??= {};
        window.__muster.world.changes[workspace] = {
          base: "main",
          uncommitted: [],
          branch: [],
          commits: [],
          more_commits: 0,
        };
        window.__muster.world.timelines ??= {};
        window.__muster.world.timelines[workspace] = { entries: [] };
      },
      { command, workspace },
    );
    await page.keyboard.press("r");
    await expect(page.getByText(empty)).toBeVisible();
    if (command === "WorkspaceChanges") {
      await page.keyboard.press("Tab");
      await expect(page.getByText("Nothing has changed here since main.")).toBeVisible();
    }
  });
}

test("ARC-25 a 5,000-line diff opens at once, draws under 100 lines, and scrolls with End, Home, n and p", async ({
  page,
}) => {
  await open(page);
  await page.evaluate(() => {
    const half = (from: number) =>
      Array.from({ length: 2500 }, (_, n) => `+line ${n + from}`).join("\n");
    window.__muster.world.diffs ??= {};
    window.__muster.world.diffs["01JD0000000000000000000011:uncommitted:src/link.rs"] = {
      text: `@@ -0,0 +1,2500 @@\n${half(0)}\n@@ -0,0 +2501,2500 @@\n${half(2500)}`,
      cut: false,
    };
  });
  const start = Date.now();
  await page.getByRole("option", { name: "modified: src/link.rs" }).click();
  const diff = page.getByRole("region", { name: "Diff" });
  await expect(diff.getByText("line 0", { exact: true })).toBeVisible();
  const opened = Date.now() - start;
  test.info().annotations.push({ type: "5,000-line diff opened in", description: `${opened} ms` });
  expect(opened).toBeLessThan(1500);
  expect(await diff.locator(".mu-dl").count()).toBeLessThan(100);
  await page.keyboard.press("End");
  await expect(diff.getByText("line 4999", { exact: true })).toBeVisible();
  expect(await diff.locator(".mu-dl").count()).toBeLessThan(100);
  await page.keyboard.press("Home");
  await expect(diff.getByText("line 0", { exact: true })).toBeVisible();
  await page.keyboard.press("n");
  await page.keyboard.press("n");
  await expect(diff.getByText("line 2500", { exact: true })).toBeVisible();
  await page.keyboard.press("p");
  await expect(diff.getByText("line 0", { exact: true })).toBeVisible();
});

test("ARC-24 binary, empty, cut and refused diffs are said in words", async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    window.__muster.world.diffs ??= {};
    window.__muster.world.diffs["01JD0000000000000000000011:uncommitted:src/link.rs"] = {
      text: "diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ",
      cut: false,
    };
  });
  await page.getByRole("option", { name: "modified: src/link.rs" }).click();
  await expect(page.getByText("binary: git shows no lines for it.")).toBeVisible();
  await page.evaluate(() => {
    window.__muster.world.diffs ??= {};
    window.__muster.world.diffs["01JD0000000000000000000011:uncommitted:src/link.rs"] = {
      text: "",
      cut: true,
    };
  });
  await page.keyboard.press("r");
  await expect(page.getByText("Git shows no difference in its text.")).toBeVisible();
  await expect(page.getByText("… cut here: the rest is longer than Muster shows.")).toBeVisible();
  await page.evaluate(() => {
    window.__muster.failNext("FileDiff", "src/link.rs is no longer changed there", "not_found");
  });
  await page.keyboard.press("r");
  await expect(page.getByRole("alert")).toHaveText(
    "src/link.rs is no longer changed there r asks again.",
  );
});

test("ARC-24 overlap and conflict banners, worded as the TUI's, with the other work a key away", async ({
  page,
}) => {
  await open(page);
  const banner = page.locator(".mu-banner");
  await expect(banner).toHaveText(
    "!Overlap with ⎇ fix/resize-under-load: both change crates/muster-client/src/connection.rs. Git could merge them.See its changes",
  );
  await expect(banner).toHaveClass(/mu-banner--overlap/);
  await expect(statusBar(page)).toContainText("the other work");
  await page.evaluate(
    ({ workspace, other }) =>
      window.__muster.updateWorkspace(workspace, {
        clashes: [
          {
            with: other,
            files: 4,
            conflicts: 3,
            shown: [{ path: "src/link.rs", conflict: true }],
          },
        ],
      }),
    { workspace, other },
  );
  await expect(banner).toHaveText(
    "!Conflict with ⎇ fix/resize-under-load: would conflict in src/link.rs and 2 more.See its changes",
  );
  await expect(banner).not.toHaveClass(/mu-banner--overlap/);
  await page.evaluate(
    (workspace) => window.__muster.updateWorkspace(workspace, { clashes: [] }),
    workspace,
  );
  await expect(banner).toBeHidden();
  await page.evaluate(
    ({ workspace, other }) =>
      window.__muster.updateWorkspace(workspace, {
        clashes: [{ with: other, files: 1, conflicts: 0, shown: [{ path: "a", conflict: false }] }],
      }),
    { workspace, other },
  );
  // The other workspace's session, in All sessions (its tree is feature/desktop-sessions').
  await page.keyboard.press("1");
  await expect(page.getByRole("heading", { name: "All sessions" })).toBeVisible();
});

test("ARC-25 asked again as the workspace moves, without polling", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("option", { name: "modified: src/link.rs" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Turns 4" })).toBeVisible();
  // What was asked so far, This turn's timeline among it.
  const timelines = (await sent(page, "Timeline")).length;
  expect((await sent(page, "WorkspaceChanges")).length).toBe(1);
  await page.evaluate((workspace) => {
    const changes = window.__muster.world.changes?.[workspace];
    if (changes)
      changes.uncommitted = [{ path: "fresh.rs", from: "old.rs", kind: "renamed", lines: null }];
    window.__muster.updateWorkspace(workspace, {
      timeline: 5,
      changes: { files: 3, added: 97, removed: 2 },
    });
  }, workspace);
  await expect(page.getByRole("option", { name: "renamed: old.rs → fresh.rs" })).toHaveText(
    "Rold.rs → fresh.rsbinary",
  );
  await expect.poll(async () => (await sent(page, "Timeline")).length).toBe(timelines + 1);
  expect((await sent(page, "WorkspaceChanges")).length).toBe(2);
});

test("ARC-25 5,000 files not committed: a few dozen rows drawn, End and Home and the wheel stay smooth", async ({
  page,
}) => {
  await open(page, async (page) => {
    await page.evaluate((workspace) => {
      const changes = window.__muster.world.changes?.[workspace];
      if (changes)
        changes.uncommitted = Array.from({ length: 5000 }, (_, n) => ({
          path: `src/file${String(n).padStart(4, "0")}.rs`,
          from: null,
          kind: "modified" as const,
          lines: { added: 1, removed: 1 },
        }));
    }, workspace);
  });
  const files = page.getByRole("listbox", { name: "Files not committed" });
  await expect(files.getByRole("option").first()).toHaveAccessibleName("modified: src/file0000.rs");
  expect(await files.getByRole("option").count()).toBeLessThan(80);
  // Each row is where its place says, so the rows drawn meet the spacers exactly.
  const pitch = await files
    .getByRole("option")
    .evaluateAll(
      (rows) =>
        (rows[1]?.getBoundingClientRect().top ?? 0) - (rows[0]?.getBoundingClientRect().top ?? 0),
    );
  expect(pitch).toBe(26);

  await page.keyboard.press("End");
  const last = files.getByRole("option", { name: "modified: src/file4999.rs" });
  await expect(last).toBeFocused();
  await expect(last).toBeInViewport();
  expect(await files.getByRole("option").count()).toBeLessThan(80);
  await page.keyboard.press("Home");
  await expect(files.getByRole("option", { name: "modified: src/file0000.rs" })).toBeFocused();

  // The wheel draws the rows it brings into sight, without a gap.
  await files.hover();
  await page.mouse.wheel(0, 26 * 2000);
  await expect(files.getByRole("option", { name: "modified: src/file2000.rs" })).toBeInViewport();
  expect(await files.getByRole("option").count()).toBeLessThan(80);
  await files.getByRole("option", { name: "modified: src/file2003.rs" }).click();
  await expect(page.locator(".mu-diffview-head")).toContainText("src/file2003.rs");
});

test("ARC-24 ? lists the keys in the changes, the person's, and ⇧Q goes back to the terminal", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.press("?");
  const keys = page.getByRole("dialog", { name: "Every key" });
  await expect(keys.getByRole("heading", { name: "In the changes" })).toBeVisible();
  await expect(keys.getByRole("listitem").filter({ hasText: "ask git again" })).toHaveText(
    "ask git againr",
  );
  await expect(
    keys.getByRole("listitem").filter({ hasText: "on a turn, put the files back" }),
  ).toContainText("z");
  await page.keyboard.press("Escape");
  await expect(keys).toBeHidden();
  await page.keyboard.press("Shift+Q");
  await expect(page.getByLabel("Terminal", { exact: true })).toBeVisible();
});

test("ARC-24 the changes hold the keys: one they do not answer never reaches the list beneath", async ({
  page,
}) => {
  await open(page);
  await page.locator("body").focus();
  // The queue's own keys: x closes the session, R renames it, n starts another, j moves on.
  for (const key of ["x", "Shift+R", "n"]) await page.keyboard.press(key);
  await expect(page.locator("dialog[open]")).toHaveCount(0);
  await expect(page.locator('input[name="session-name"]')).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Not committed/ })).toBeVisible();
  // Its own still answer: d goes back to the terminal.
  await page.keyboard.press("d");
  await expect(page.getByRole("button", { name: /^Not committed/ })).toHaveCount(0);
});
