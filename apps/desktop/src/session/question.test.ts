import { describe, expect, it } from "vitest";
import { plainText } from "./question";

describe("ARC-24 what a session waits for, as its banner shows it", () => {
  it("PRD-06 shows no escape or control character a program wrote", () => {
    expect(
      plainText("\u001b]8;;https://x\u0007link\u001b]8;;\u0007 \u001b[31mred\u001b[0m\u0007"),
    ).toBe("link red");
  });
});
