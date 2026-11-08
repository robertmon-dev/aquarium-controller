import { bem } from "../../utility/bem";
import type { TagVariant, TagSize } from "./types";

interface StyleProps {
  variant: TagVariant;
  size: TagSize;
  clickable?: boolean;
  disabled?: boolean;
}

export const getTagClasses = ({
  variant,
  size,
  clickable,
  disabled,
}: StyleProps): string => {
  return bem("tag", [
    variant,
    size,
    clickable && !disabled ? "clickable" : null,
    disabled ? "disabled" : null,
  ]);
};
