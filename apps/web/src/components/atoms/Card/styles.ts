import type { CardProps } from "./types";
import { bem } from "../../utility/bem";

type StyleProps = Pick<
  CardProps,
  "variant" | "padding" | "interactive" | "className" | "width"
>;

export const getCardClasses = ({
  variant = "elevated",
  padding = "md",
  width = "auto",
  interactive,
  className,
}: StyleProps): string => {
  return bem(
    "card",
    [
      variant,
      `padding-${padding}`,
      width !== "auto" && `width-${width}`,
      interactive && "interactive",
    ],
    className,
  );
};
