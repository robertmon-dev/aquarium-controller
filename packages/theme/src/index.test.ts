import { describe, expect, it } from "vitest";
import { withAlpha } from "./color";
import {
  alertStyle,
  badgeContainerStyle,
  badgeLabelStyle,
  buttonContainerStyle,
  buttonGradient,
  cardStyle,
  chipContainerStyle,
  colors,
  inputStyles,
  radius,
  typography,
} from "./index";

const TINTED_BADGES = [
  ["primary", colors.primary],
  ["secondary", colors.textMuted],
  ["warning", colors.warning],
  ["success", colors.success],
  ["danger", colors.danger],
  ["info", colors.info],
] as const;

describe("the tinted surface rule", () => {
  it.each(TINTED_BADGES)(
    "fills badge %s at 10%% of its accent and borders it at 20%%",
    (variant, accent) => {
      const style = badgeContainerStyle(variant, "md");

      expect(style.backgroundColor).toBe(withAlpha(accent, 0.1));
      expect(style.borderWidth).toBe(1);
      expect(style.borderColor).toBe(withAlpha(accent, 0.2));
    },
  );

  it.each(["info", "success", "warning", "danger"] as const)(
    "borders the %s alert instead of barring it on the left",
    (variant) => {
      const style = alertStyle(variant);

      expect(style.borderWidth).toBe(1);
      expect(style.borderLeftWidth).toBeUndefined();
      expect(style.borderRadius).toBe(radius.md);
    },
  );
});

describe("badge typography", () => {
  it("carries the weight and tracking the reference uses", () => {
    const style = badgeLabelStyle("primary", "md");

    expect(style.fontWeight).toBe("600");
    expect(style.letterSpacing).toBeGreaterThan(0);
  });

  it("reads outline as main text and secondary as muted", () => {
    expect(badgeLabelStyle("outline", "md").color).toBe(colors.textMain);
    expect(badgeLabelStyle("secondary", "md").color).toBe(colors.textMuted);
  });
});

describe("the input field", () => {
  it("reserves the focus ring with a transparent border at rest", () => {
    expect(inputStyles.field.borderWidth).toBe(2);
    expect(inputStyles.field.borderColor).toBe(withAlpha(colors.primary, 0));
  });

  it("sits above the card it lives on rather than sunken below it", () => {
    expect(inputStyles.field.backgroundColor).toBe("#2c3148");
  });

  it("shows a soft primary ring on focus, not a full strength border", () => {
    expect(inputStyles.fieldFocused.borderColor).toBe(
      withAlpha(colors.primary, 0.3),
    );
  });

  it("rounds to the same radius as the reference field", () => {
    expect(inputStyles.field.borderRadius).toBe(radius.mdL);
    expect(inputStyles.field.minHeight).toBe(44);
  });

  it("indents label and helper the way the reference does", () => {
    expect(inputStyles.label.marginLeft).toBeGreaterThan(0);
    expect(inputStyles.helper.marginLeft).toBeGreaterThan(0);
  });
});

describe("filled buttons", () => {
  it.each(["success", "purple"] as const)(
    "gives %s a gradient with two distinct stops",
    (variant) => {
      const gradient = buttonGradient(variant);

      expect(gradient).not.toBeNull();
      expect(gradient?.[0]).not.toBe(gradient?.[1]);
    },
  );

  it("leaves primary flat, as both of the reference's stops are the same colour", () => {
    const gradient = buttonGradient("primary");

    expect(gradient?.[0]).toBe(gradient?.[1]);
  });

  it.each(["primary", "success", "purple"] as const)(
    "lifts %s with a shadow rather than a bare hairline",
    (variant) => {
      const style = buttonContainerStyle({
        variant,
        size: "md",
        fullWidth: false,
        isIcon: false,
        pressed: false,
        disabled: false,
      });

      expect(style.boxShadow).toBeDefined();
    },
  );
});

describe("cards", () => {
  it("borders the elevated variant, which carries the system's slate rim", () => {
    const style = cardStyle({
      variant: "elevated",
      padding: "md",
      width: "auto",
    });

    expect(style.borderColor).toBe(withAlpha(colors.textMuted, 0.5));
  });

  it("keeps a transparent border on variants that show none", () => {
    const style = cardStyle({
      variant: "transparent",
      padding: "md",
      width: "auto",
    });

    expect(style.borderWidth).toBe(2);
  });
});

describe("chips", () => {
  it("marks the selected chip with the primary accent", () => {
    const selected = chipContainerStyle("dry", true);

    expect(selected.borderColor).toBe(colors.primary);
    expect(selected.borderWidth).toBe(2);
  });

  it("leaves an unselected chip on its own variant colours", () => {
    const dry = chipContainerStyle("dry", false);

    expect(dry.borderColor).toBe(colors.border);
    expect(dry.borderWidth).toBe(1);
  });
});
