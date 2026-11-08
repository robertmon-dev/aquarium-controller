import type { InputProps } from './types';
import { bem } from '../../utility/bem';

type StyleProps = {
  error?: InputProps['error'] | undefined;
  fullWidth?: InputProps['fullWidth'] | undefined;
  disabled?: InputProps['disabled'] | undefined;
  className?: InputProps['className'] | undefined;
};

export const getInputWrapperClasses = ({
  error,
  fullWidth,
  disabled,
}: StyleProps): string => {
  return bem('input-wrapper', [
    !!error && 'error',
    disabled && 'disabled',
    fullWidth && 'full-width'
  ]);
};
