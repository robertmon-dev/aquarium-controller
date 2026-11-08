import React from "react";

export type CardVariant =
  | "elevated"
  | "outlined"
  | "flat"
  | "transparent"
  | "levitating"
  | "levitating-transparent"
  | "floating"
  | "contact";
export type CardPadding = "none" | "sm" | "md" | "lg";
export type CardWidth = "auto" | "wide" | "full";

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
  variant?: CardVariant;
  padding?: CardPadding;
  width?: CardWidth;
  interactive?: boolean;
}
