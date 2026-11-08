export const bem = (
  baseClass: string,
  modifiers: (string | boolean | undefined | null)[],
  extraClass: string = ''
): string => {
  const classes = [baseClass];

  modifiers.forEach((mod) => {
    if (mod && typeof mod === 'string') {
      classes.push(`${baseClass}--${mod}`);
    }
  });

  if (extraClass) {
    classes.push(extraClass);
  }

  return classes.join(' ');
};
