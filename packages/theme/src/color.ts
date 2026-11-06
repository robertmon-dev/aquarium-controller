const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;

export function withAlpha(color: string, alpha: number): string {
  const match = HEX.exec(color.trim());
  if (!match) throw new Error(`withAlpha expects a hex colour, got "${color}"`);

  const digits = match[1] ?? "";
  const full =
    digits.length === 3 ? digits.replace(/./g, (d) => `${d}${d}`) : digits;
  const value = Number.parseInt(full, 16);
  const clamped = Math.min(1, Math.max(0, alpha));

  return `rgba(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}, ${clamped})`;
}
