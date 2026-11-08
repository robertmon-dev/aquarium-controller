import { useEffect } from "react";

export const useModalEffects = (isOpen: boolean, onClose?: () => void) => {
  useEffect(() => {
    if (!isOpen) return;

    const originalStyle = window.getComputedStyle(document.body).overflow;

    document.body.style.overflow = "hidden";

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose?.();
      }
    };

    window.addEventListener("keydown", handleEscape);

    return () => {
      const openedModals = document.querySelectorAll(".modal-wrapper").length;
      if (openedModals <= 1) {
        document.body.style.overflow = originalStyle;
      }
      window.removeEventListener("keydown", handleEscape);
    };
  }, [isOpen, onClose]);
};
