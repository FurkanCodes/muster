import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

declare global {
  interface Window {
    __muster: MockBridge;
  }
}
const workspace = "01JD0000000000000000000011";

async function open(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Changes", exact: true }).click();
}

test("ARC-24 files, branch and turns send the daemon's commands, with keys", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "modified: src/link.rs" }).click();
  const diff = page.getByRole("region", { name: "Diff" });
  await expect(diff).toContainText("use muster_client::{Connection, Received};");
  await expect(diff.locator(".mu-dl--del").first()).toContainText("Connection;");
  await page.keyboard.press("h");
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__muster.sent))
    .toContainEqual({ FileDiff: { workspace, part: "uncommitted", path: "src/main.rs" } });
  await page.keyboard.press("Tab");
  await expect(page.getByText("Add the desktop crate")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__muster.sent))
    .toContainEqual({ FileDiff: { workspace, part: "branch", path: "src/link.rs" } });
  await page.keyboard.press("Tab");
  await expect(page.getByRole("option", { name: /Codex · turn 3/ })).toBeVisible();
  await expect(page.getByText("Edits between turns")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__muster.sent))
    .toContainEqual({ EntryDiff: { workspace, number: 4 } });
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Terminal", { exact: true })).toBeVisible();
});

for (const command of ["WorkspaceChanges", "Timeline"] as const) {
  test(`ARC-24 ${command}: loading, refusal, retry and empty`, async ({ page }) => {
    await page.goto("/");
    await page.evaluate((command) => window.__muster.held.add(command), command);
    await page.getByRole("button", { name: "Changes", exact: true }).click();
    if (command === "Timeline") await page.getByRole("button", { name: /^Turns/ }).click();
    await expect(
      page.getByText(command === "Timeline" ? "Reading the turns…" : "Asking git…"),
    ).toBeVisible();
    await page.evaluate((command) => {
      window.__muster.errors[command] = {
        code: "internal",
        message: "Git could not read this workspace.",
      };
      window.__muster.release(command);
    }, command);
    await expect(page.getByRole("alert")).toContainText("Git could not read");
    await page.evaluate((command) => {
      delete window.__muster.errors[command];
      window.__muster.replies[command] =
        command === "Timeline"
          ? { Timeline: { entries: [] } }
          : {
              Changes: { base: "main", uncommitted: [], branch: [], commits: [], more_commits: 0 },
            };
    }, command);
    await page.keyboard.press("r");
    await expect(
      page.getByText(
        command === "Timeline"
          ? "None kept yet: each agent's turn here is, from now on."
          : "Nothing is waiting to be committed.",
      ),
    ).toBeVisible();
    if (command === "WorkspaceChanges") {
      await page.keyboard.press("Tab");
      await expect(page.getByText("Nothing has changed here since main.")).toBeVisible();
    }
  });
}

test("ARC-25 a 5,000-line diff stays bounded and scrolls with End, Home, n and p", async ({
  page,
}) => {
  await open(page);
  await page.evaluate(() => {
    window.__muster.replies.FileDiff = {
      FileDiff: {
        text: `@@ -0,0 +1,2500 @@\n${Array.from({ length: 2500 }, (_, n) => `+line ${n}`).join("\n")}\n@@ -0,0 +2501,2500 @@\n${Array.from({ length: 2500 }, (_, n) => `+line ${n + 2500}`).join("\n")}`,
        cut: false,
      },
    };
  });
  const start = Date.now();
  await page.getByRole("option", { name: "modified: src/link.rs" }).click();
  const diff = page.getByRole("region", { name: "Diff" });
  await expect(diff.getByText("line 0", { exact: true })).toBeVisible();
  expect(Date.now() - start).toBeLessThan(2000);
  expect(await diff.locator(".mu-dl").count()).toBeLessThan(100);
  await diff.focus();
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
  await page.keyboard.press("c");
  await expect(page.getByLabel("Terminal", { exact: true })).toBeVisible();
});

test("ARC-24 binary, cut and failed diffs say what happened", async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    window.__muster.replies.FileDiff = {
      FileDiff: { text: "Binary files a/image.png and b/image.png differ", cut: false },
    };
  });
  await page.getByRole("option", { name: "modified: src/link.rs" }).click();
  await expect(page.getByText(/binary —/)).toBeVisible();
  await page.evaluate(() => {
    window.__muster.replies.FileDiff = { FileDiff: { text: "", cut: true } };
  });
  await page.keyboard.press("r");
  await expect(page.getByText("Git shows no difference in its text.")).toBeVisible();
  await expect(page.getByText(/cut here:/)).toBeVisible();
  await page.evaluate(() => {
    window.__muster.errors.FileDiff = {
      code: "not_found",
      message: "src/link.rs is not among the files changed there",
    };
  });
  await page.keyboard.press("r");
  await expect(page.getByRole("alert")).toContainText("not among the files");
});

test("ARC-24 clashes follow workspace patches and refresh follows the summary", async ({
  page,
}) => {
  await open(page);
  await expect(page.getByText("Overlap", { exact: true })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Overlap" })).toContainText(
    "changes them too:",
  );
  await page.evaluate(
    (workspace) =>
      window.__muster.updateWorkspace(workspace, {
        clashes: [
          {
            with: "01JD0000000000000000000012",
            files: 4,
            conflicts: 3,
            shown: [{ path: "src/link.rs", conflict: true }],
          },
        ],
      }),
    workspace,
  );
  await expect(page.getByText("Conflict", { exact: true })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Conflict" })).toContainText(
    "would conflict in src/link.rs and 2 more",
  );
  await page.evaluate((workspace) => {
    const changes = window.__muster.world.changes[workspace];
    if (changes)
      changes.uncommitted = [{ path: "fresh.rs", from: "old.rs", kind: "renamed", lines: null }];
    window.__muster.updateWorkspace(workspace, { changes: { files: 1, added: 0, removed: 0 } });
  }, workspace);
  await expect(page.getByRole("option", { name: "renamed: old.rs → fresh.rs" })).toBeVisible();
  await expect(page.getByText("binary or too large to count")).toBeVisible();
});
