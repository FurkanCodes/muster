// A control's tooltip as a person finds it: the pointer rests on it, the
// tooltip shows, and the pointer moves away (ui/Tooltip.tsx).

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect } from "vitest";

export async function tooltipOf(element: HTMLElement): Promise<string> {
  await userEvent.hover(element);
  const tip = await screen.findByRole("tooltip", {}, { timeout: 2000 });
  const said = tip.textContent ?? "";
  await userEvent.unhover(element);
  await waitFor(() => expect(screen.queryByRole("tooltip")).toBeNull());
  return said;
}
