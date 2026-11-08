import { memoryAdapter } from "@aquarium/i18n/adapter";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChannelSliders } from "./ChannelSliders";

function renderSliders(onChange = vi.fn(), onCommit = vi.fn()) {
  render(
    <LocaleProvider adapter={memoryAdapter({ stored: "en", system: "en" })}>
      <ChannelSliders
        levels={{ white_pct: 40, rgb_pct: 20 }}
        onChange={onChange}
        onCommit={onCommit}
      />
    </LocaleProvider>,
  );
  return { onChange, onCommit };
}

describe("ChannelSliders", () => {
  it("renders one slider per channel at its current value", async () => {
    renderSliders();

    await waitFor(() => expect(screen.getAllByRole("slider").length).toBe(2));
    expect(screen.getByText("40%")).toBeTruthy();
    expect(screen.getByText("20%")).toBeTruthy();
  });

  it("reports a moved channel without committing until release", async () => {
    const { onChange, onCommit } = renderSliders();

    await waitFor(() => expect(screen.getAllByRole("slider").length).toBe(2));
    const [white] = screen.getAllByRole("slider");
    if (!white) throw new Error("no slider rendered");

    fireEvent.change(white, { target: { value: "80" } });

    expect(onChange).toHaveBeenCalledWith("white_pct", 80);
    expect(onCommit).not.toHaveBeenCalled();

    fireEvent.mouseUp(white);
    expect(onCommit).toHaveBeenCalledTimes(1);
  });
});
