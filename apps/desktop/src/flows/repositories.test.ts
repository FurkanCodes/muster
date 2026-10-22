import { describe, expect, it } from "vitest";
import { folderOf, repositoryName, shownRepositories } from "./repositories";

describe("PRD §6a cloning from GitHub, as the daemon names repositories", () => {
  it.each([
    ["FurkanCodes/muster", "FurkanCodes/muster"],
    ["  o/app ", "o/app"],
    ["https://github.com/o/app.git", "o/app"],
    ["https://github.com/o/app/", "o/app"],
    ["git@github.com:o/my_app.v2.git", "o/my_app.v2"],
  ])("%s names %s", (typed, name) => expect(repositoryName(typed)).toBe(name));

  it.each([
    "",
    "app",
    "o/app/extra",
    "-o/app",
    "o/--upload-pack=x",
    "../app",
    "o/.hidden",
    "https://gitlab.com/o/app",
  ])("%s names none", (typed) => expect(repositoryName(typed)).toBeNull());

  it("clones into a folder of its name, and narrows by name first", () => {
    expect(folderOf("o/app")).toBe("app");
    const listed = [
      { name: "o/site", description: "The app's website", private: false },
      { name: "o/app", description: null, private: true },
      { name: "o/tools", description: null, private: false },
    ];
    expect(shownRepositories(listed, "APP").map((one) => one.name)).toEqual(["o/app", "o/site"]);
    expect(shownRepositories(listed, "")).toHaveLength(3);
  });
});
