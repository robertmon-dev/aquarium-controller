import type { ButtonProps } from './types';
import { bem } from '../../utility/bem';

type StyleProps = Pick<ButtonProps, 'variant' | 'size' | 'fullWidth' | 'isLoading' | 'className' | 'isIcon'>;

export const getButtonClasses = ({
  variant = 'primary',
  size = 'md',
  fullWidth,
  isLoading,
  className,
  isIcon,
}: StyleProps): string => {
  return bem(
    'btn',
    [
      variant,
      size,
      fullWidth && 'full-width',
      isLoading && 'loading',
      isIcon && 'icon'
    ],
    className
  );
};
