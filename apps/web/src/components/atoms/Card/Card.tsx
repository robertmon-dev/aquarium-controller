import { forwardRef } from "react";
import type { CardProps } from "./types";
import { getCardClasses } from "./styles";
import "./Card.scss";

export const Card = forwardRef<HTMLDivElement, CardProps>((props, ref) => {
  const {
    variant = "elevated",
    padding = "md",
    width = "auto",
    interactive = false,
    hover = true,
    className,
    children,
    ...rest
  } = props;
  const classes = getCardClasses({
    variant,
    padding,
    width,
    interactive,
    hover,
    className,
  });

  return (
    <div ref={ref} className={classes} {...rest}>
      {children}
    </div>
  );
});

Card.displayName = "Card";
