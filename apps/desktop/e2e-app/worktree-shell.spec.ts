import { $, expect } from "@wdio/globals";
import { muster, project, sessions, status, until } from "./support/muster";
import { branches, repository } from "./support/repos";
import { button, field, option, press, selectRow } from "./support/window";

// A shell started in a worktree that is already there, over the real daemon
// (PRODUCT.md §6a, PRD-03): from its line with n, from its card's + on the
// project's page, and chosen under Where. Each starts the shell in that
// worktree, on its branch; none makes a worktree or a branch of its own.

const PROJECT = "shop";
const BRANCH = "feat/cart";
let repo = "";

const box = () => $(`//section[@aria-label="${PROJECT}"]//form[@aria-label="Start work"]`);

/** Chooses the option holding `name` in the box's select labelled `label`. */
async function choose(label: string, name: string) {
  await box().$(`.//button[@aria-label="${label}"]`).click();
  await option($(`//*[@role="listbox"][@aria-label="${label}"]`), name).click();
}

const worktrees = () => project(PROJECT)?.workspaces.filter((one) => one.kind === "worktree") ?? [];

/** Starts a shell from the box as it stands, and checks it ran in the worktree on BRANCH, nothing new made. */
async function startsInTheWorktree() {
  const before = new Set(sessions().map((one) => one.id));
  const branchesBefore = branches(repo);
  await choose("Agent", "A shell");
  await button(box(), "Start a shell").click();
  const made = await until(
    () => sessions().find((one) => !before.has(one.id)),
    "the shell to start",
  );
  expect(made.harness).toBe("shell");
  expect(made.branch).toBe(BRANCH);
  expect(made.workspace).toBe(worktrees()[0]?.id);
  expect(worktrees()).toHaveLength(1);
  expect(branches(repo)).toEqual(branchesBefore);
  await press("Q");
}

describe("A shell in a worktree that is already there", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository(PROJECT);
    muster("project", "add", repo);
    muster("workspace", "add", "--project", PROJECT, "--branch", BRANCH);
    await until(() => worktrees().length === 1, "the worktree");
  });

  it("PRD-03 n on the worktree's line starts the shell in it, with no new worktree or branch", async () => {
    await selectRow(BRANCH);
    await press("n");
    await expect(box()).toBeDisplayed();
    await expect(box().$('.//button[@aria-label="Where"]')).toHaveText(
      expect.stringContaining(BRANCH),
    );
    await startsInTheWorktree();
  });

  it("PRD-03 the + on the worktree's card starts the shell in it", async () => {
    await selectRow(PROJECT);
    await $(`//button[@aria-label="Start work in ${BRANCH}"]`).click();
    await expect(box().$('.//button[@aria-label="Where"]')).toHaveText(
      expect.stringContaining(BRANCH),
    );
    await startsInTheWorktree();
  });

  it("PRD-03 the worktree chosen under Where starts the shell in it", async () => {
    await selectRow(PROJECT);
    await press("n");
    await expect(box()).toBeDisplayed();
    await choose("Where", BRANCH);
    await expect(field(box(), "Branch")).not.toBeExisting();
    await startsInTheWorktree();
  });
});
