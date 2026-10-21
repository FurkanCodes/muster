import { describe, expect, it } from "vitest";
import { rgbOf, terminalColours } from "./colours";

describe("ARC-24 the terminals' colours, as the daemon is told them", () => {
  it("reads a token's colour in sixteen bits a channel, as X11 scales it", () => {
    expect(rgbOf("#13131b")).toEqual({ red: 0x1313, green: 0x1313, blue: 0x1b1b });
    expect(rgbOf(" #fff ")).toEqual({ red: 0xffff, green: 0xffff, blue: 0xffff });
    expect(rgbOf("oklch(0.5 0.1 200)")).toBeNull();
    expect(rgbOf("")).toBeNull();
  });

  it("tells the theme's text and background and xterm's sixteen colours", () => {
    const element = document.createElement("div");
    element.style.setProperty("--text", "#d8dae5");
    element.style.setProperty("--surface-term", "#13131b");
    document.body.append(element);
    const colours = terminalColours(element);
    expect(colours.foreground).toEqual({ red: 0xd8d8, green: 0xdada, blue: 0xe5e5 });
    expect(colours.background).toEqual({ red: 0x1313, green: 0x1313, blue: 0x1b1b });
    expect(colours.palette).toHaveLength(16);
    expect(colours.palette.every(Boolean)).toBe(true);
    element.remove();
  });
});
