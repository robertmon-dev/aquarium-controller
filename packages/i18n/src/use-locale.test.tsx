import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { memoryAdapter, type LocaleAdapter } from "./adapter";
import { LocaleProvider, useLocale } from "./use-locale";

function Probe() {
  const { locale, t } = useLocale();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <span data-testid="label">{t("language.label")}</span>
    </div>
  );
}

function renderWith(adapter: LocaleAdapter) {
  return render(
    <LocaleProvider adapter={adapter}>
      <Probe />
    </LocaleProvider>,
  );
}

describe("LocaleProvider", () => {
  it("takes the stored locale over the system language", async () => {
    renderWith(memoryAdapter({ stored: "en", system: "pl" }));

    await waitFor(() =>
      expect(screen.getByTestId("locale").textContent).toBe("en"),
    );
  });

  it("falls back to the system language when nothing is stored", async () => {
    renderWith(memoryAdapter({ stored: null, system: "pl" }));

    await waitFor(() =>
      expect(screen.getByTestId("locale").textContent).toBe("pl"),
    );
    expect(screen.getByTestId("label").textContent).toBe("język");
  });

  it("falls back to english when the adapter throws", async () => {
    const broken: LocaleAdapter = {
      load: () => Promise.reject(new Error("no keychain")),
      save: () => Promise.resolve(),
      systemLanguage: () => "de",
    };

    renderWith(broken);

    await waitFor(() =>
      expect(screen.getByTestId("locale").textContent).toBe("en"),
    );
  });

  it("persists a chosen locale through the adapter", async () => {
    const adapter = memoryAdapter({ stored: null, system: "en" });

    function Switcher() {
      const { setLocale } = useLocale();
      return <button onClick={() => void setLocale("pl")}>switch</button>;
    }

    render(
      <LocaleProvider adapter={adapter}>
        <Probe />
        <Switcher />
      </LocaleProvider>,
    );

    screen.getByText("switch").click();

    await waitFor(() => expect(adapter.saved).toEqual(["pl"]));
  });

  it("refuses to be used outside the provider", () => {
    expect(() => render(<Probe />)).toThrow(
      "useLocale must be used inside LocaleProvider",
    );
  });
});
