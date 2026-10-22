// A Select's list as a person meets it (ui/Select.tsx, on Base UI's
// select): it opens a frame after the press and takes the keys as it does,
// and goes a frame after a choice.

import { screen, waitFor } from "@testing-library/react";
import { expect } from "vitest";

/** The open list named `name`, once it has the keys. */
export async function openList(name: string | RegExp): Promise<HTMLElement> {
  const list = await screen.findByRole("listbox", { name });
  await waitFor(() => expect(list.contains(document.activeElement)).toBe(true));
  return list;
}

/** Once no list is open. */
export async function listGone(): Promise<void> {
  await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
}
