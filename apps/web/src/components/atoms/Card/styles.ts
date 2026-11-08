import type { CardProps } from "./types";
import { bem } from "../../utility/bem";

type StyleProps = Pick<
  CardProps,
  "variant" | "padding" | "interactive" | "className" | "width" | "hover"
>;

export const getCardClasses = ({
  variant = "elevated",
  padding = "md",
  width = "auto",
  interactive,
  className,
  hover,
}: StyleProps): string => {
  return bem(
    "card",
    [
      variant,
      `padding-${padding}`,
      width !== "auto" && `width-${width}`,
      interactive && "interactive",
      hover && "hover",
    ],
    className,
  );
};
