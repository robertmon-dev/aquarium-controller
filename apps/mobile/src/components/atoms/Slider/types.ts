export interface SliderProps {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  minimumValue?: number;
  maximumValue?: number;
  step?: number;
  disabled?: boolean;
}
