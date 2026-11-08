import type { Ionicons } from "@expo/vector-icons";

export interface DeviceNoticeProps {
  /** Typed against the glyph map, so a wrong name is a compile error. */
  icon: keyof typeof Ionicons.glyphMap;
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}
