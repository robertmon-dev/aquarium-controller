import type { Ionicons } from "@expo/vector-icons";

export interface DeviceNoticeProps {
  icon: keyof typeof Ionicons.glyphMap;
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}
