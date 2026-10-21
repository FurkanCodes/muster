import { browser } from "@wdio/globals";

// Drags by the pointer. The embedded WebDriver server makes mouse events
// only, and the window's drags (panels' heads, the lines between them)
// follow pointer events, which a browser makes from a real mouse but not
// from a script's: so the drag is made here as pointer events, at the
// window's own coordinates, each sent where pointer capture would send it,
// to the element pressed.

/** Sends one pointer event of `type` at `at` to the element `selector`; whether it was there. */
function send(selector: string, type: string, at: { x: number; y: number }, buttons: number) {
  return browser.execute(
    (selector, type, x, y, buttons) => {
      const pressed = document.querySelector(selector);
      if (!(pressed instanceof HTMLElement)) return false;
      pressed.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          composed: true,
          pointerId: 1,
          pointerType: "mouse",
          isPrimary: true,
          button: 0,
          buttons,
          clientX: x,
          clientY: y,
        }),
      );
      return true;
    },
    selector,
    type,
    at.x,
    at.y,
    buttons,
  );
}

/**
 * Presses the middle of the element `selector`, moves in `steps` to `x`,
 * `y` (the window's coordinates) and lets go there; or, with `cancel`,
 * presses Escape before letting go. Each event goes a frame after the
 * last, as a hand's would, so the window draws between them.
 */
export async function dragTo(
  selector: string,
  x: number,
  y: number,
  { steps = 8, cancel = false } = {},
): Promise<void> {
  const from = await middleOf(selector);
  const frame = () => browser.pause(20);
  if (!(await send(selector, "pointerdown", from, 1))) throw new Error(`nothing at ${selector}`);
  for (let step = 1; step <= steps; step += 1) {
    await frame();
    const part = step / steps;
    const at = { x: from.x + (x - from.x) * part, y: from.y + (y - from.y) * part };
    await send(selector, "pointermove", at, 1);
  }
  await frame();
  if (cancel) {
    await browser.execute(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    await frame();
  }
  await send(selector, "pointerup", { x, y }, 0);
}

/** The middle of the element `selector`, in the window's coordinates. */
export async function middleOf(selector: string): Promise<{ x: number; y: number }> {
  const at = await browser.execute((selector) => {
    const box = document.querySelector(selector)?.getBoundingClientRect();
    return box ? { x: box.left + box.width / 2, y: box.top + box.height / 2 } : null;
  }, selector);
  if (!at) throw new Error(`nothing at ${selector}`);
  return at;
}

/** The box of the element `selector`, in the window's coordinates. */
export async function boxOf(
  selector: string,
): Promise<{ x: number; y: number; width: number; height: number }> {
  const box = await browser.execute((selector) => {
    const found = document.querySelector(selector)?.getBoundingClientRect();
    return found ? { x: found.left, y: found.top, width: found.width, height: found.height } : null;
  }, selector);
  if (!box) throw new Error(`nothing at ${selector}`);
  return box;
}
