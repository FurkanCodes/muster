import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("turns on and off, and takes no press while busy or disabled, saying why", async () => {
    const onChange = vi.fn();
    const { rerender } = render(<Switch on={false} label="Notifications" onChange={onChange} />);
    await userEvent.click(screen.getByRole("switch", { name: "Notifications" }));
    expect(onChange).toHaveBeenCalledWith(true);
    rerender(<Switch on={false} label="Notifications" onChange={onChange} busy />);
    const busy = screen.getByRole("switch", { name: "Notifications" });
    expect(busy).toHaveAttribute("aria-busy", "true");
    await userEvent.click(busy);
    rerender(
      <Switch
        on={false}
        label="Notifications"
        onChange={onChange}
        disabled
        title="Turn them on above"
      />,
    );
    const off = screen.getByRole("switch", { name: "Notifications" });
    await userEvent.click(off);
    expect(onChange).toHaveBeenCalledOnce();
    expect(off).toHaveAttribute("aria-disabled", "true");
    expect(off).toHaveAttribute("title", "Turn them on above");
  });
});
