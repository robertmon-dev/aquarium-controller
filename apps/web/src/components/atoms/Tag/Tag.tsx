import { forwardRef, type KeyboardEvent } from "react";
import { X } from "lucide-react";
import type { TagProps } from "./types";
import { getTagClasses } from "./styles";
import "./Tag.scss";

export const Tag = forwardRef<HTMLDivElement, TagProps>((props, ref) => {
  const {
    children,
    variant = "default",
    size = "md",
    icon,
    onDismiss,
    clickable = false,
    disabled = false,
    maxLength = 10,
    className = "",
    onClick,
    ...rest
  } = props;

  const isClickable = (clickable || !!onClick) && !disabled;

  const tagClasses = getTagClasses({
    variant,
    size,
    clickable: isClickable,
    disabled,
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    if (disabled) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onDismiss?.();
    }
  };

  const renderContent = () => {
    if (typeof children === "string" && children.length > maxLength) {
      return `${children.slice(0, maxLength)}...`;
    }
    return children;
  };

  return (
    <div
      ref={ref}
      className={`${tagClasses} ${className}`.trim()}
      onClick={isClickable ? onClick : undefined}
      aria-disabled={disabled}
      title={typeof children === "string" ? children : undefined}
      {...rest}
    >
      {icon && <span className="tag-icon">{icon}</span>}

      <span className="tag-content">{renderContent()}</span>

      {onDismiss && (
        <span
          className="tag-dismiss"
          onClick={(e) => {
            if (disabled) return;
            e.stopPropagation();
            onDismiss();
          }}
          onKeyDown={handleKeyDown}
          role="button"
          tabIndex={disabled ? -1 : 0}
          aria-label="Remove tag"
        >
          <X size="100%" strokeWidth={2.5} />
        </span>
      )}
    </div>
  );
});

Tag.displayName = "Tag";
