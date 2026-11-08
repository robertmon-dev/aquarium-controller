export type StatusTone = "ok" | "degraded" | "off";

export type ChipVariant = "default" | "wet" | "dry" | "faulty";

export type BadgeVariant =
  | "primary"
  | "secondary"
  | "warning"
  | "success"
  | "danger"
  | "info"
  | "outline";

export type TextDisplayState =
  | "default"
  | "error"
  | "success"
  | "warning"
  | "info";

export type StreamState =
  | "idle"
  | "connecting"
  | "live"
  | "reconnecting"
  | "unauthorized";

export type Metric = {
  label: string;
  value: string;
  state?: TextDisplayState;
};
