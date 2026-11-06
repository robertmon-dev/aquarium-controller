export function formatCelsius(value: number | null): string {
  return value === null ? "—" : `${value.toFixed(2)} °C`;
}

export function formatPercentPair(white: number, rgb: number): string {
  return `${white}% / ${rgb}%`;
}
