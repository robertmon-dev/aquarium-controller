import { forwardRef } from "react";
import type { ButtonProps } from "./types";
import { getButtonClasses } from "./styles";
export type { ButtonVariant } from "./types";
import "./Button.scss";

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (props, ref) => {
    const {
      isLoading = false,
      leftIcon,
      rightIcon,
      children,
      disabled,
      isIcon = false,
      variant = "primary",
      fullWidth = false,
      className,
      ...rest
    } = props;

    const classes = getButtonClasses({
      isLoading,
      isIcon,
      variant,
      fullWidth,
      className,
    });

    return (
      <button
        ref={ref}
        className={classes}
        disabled={disabled || isLoading}
        {...rest}
      >
        {isLoading ? (
          <span className="btn-spinner" aria-hidden="true" />
        ) : (
          <>
            {isIcon ? (
              <span className="btn-icon-wrapper">
                {children || leftIcon || rightIcon}
              </span>
            ) : (
              <>
                {leftIcon && (
                  <span className="btn-icon btn-icon--left">{leftIcon}</span>
                )}

                {children && <span className="btn-label">{children}</span>}

                {rightIcon && (
                  <span className="btn-icon btn-icon--right">{rightIcon}</span>
                )}
              </>
            )}
          </>
        )}
      </button>
    );
  },
);
