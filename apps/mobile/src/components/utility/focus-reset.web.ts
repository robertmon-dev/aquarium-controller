import { useEffect } from "react";

const STYLE_ID = "aquarium-focus-reset";

export function useFocusReset(): void {
  useEffect(() => {
    if (document.getElementById(STYLE_ID)) return;

    const style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = "input:focus, textarea:focus { outline: none; }";
    document.head.appendChild(style);
  }, []);
}
