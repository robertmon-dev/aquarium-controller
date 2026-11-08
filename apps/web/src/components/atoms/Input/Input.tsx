import { forwardRef, useId } from 'react';
import type { InputProps } from './types';
import { getInputWrapperClasses } from './styles';
import './Input.scss';

export const Input = forwardRef<HTMLInputElement, InputProps>((props, ref) => {
  const {
    label,
    error,
    helperText,
    leftIcon,
    rightIcon,
    fullWidth = false,
    className = '',
    id,
    disabled,
    ...rest
  } = props;

  const generatedId = useId();
  const inputId = id || generatedId;

  const wrapperClasses = getInputWrapperClasses({ error, fullWidth, disabled });
  const errorMessage = typeof error === 'string' ? error : null;

  return (
    <div className={`input-container ${fullWidth ? 'full-width' : ''} ${className}`}>

      {label && (
        <label htmlFor={inputId} className="input-label">
          {label}
        </label>
      )}

      <div className={wrapperClasses}>

        {leftIcon && (
          <span className="input-icon input-icon--left">
            {leftIcon}
          </span>
        )}

        <input
          ref={ref}
          id={inputId}
          disabled={disabled}
          aria-invalid={!!error}
          {...rest}
        />

        {rightIcon && (
          <span className="input-icon input-icon--right">
            {rightIcon}
          </span>
        )}
      </div>

      {errorMessage ? (
        <span className="input-error-msg">{errorMessage}</span>
      ) : helperText ? (
        <span className="input-helper">{helperText}</span>
      ) : null}

    </div>
  );
});

Input.displayName = 'Input';
