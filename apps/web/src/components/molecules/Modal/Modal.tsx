import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { useId, forwardRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/atoms/Button/Button";
import { useModalTranslations } from "./useModalTranslations";
import { useModalEffects } from "./useModal";
import type { ModalProps } from "./types";
import "./Modal.scss";

const Modal = forwardRef<HTMLDivElement, ModalProps>((props, ref) => {
  const {
    open,
    title,
    children,
    onClose,
    size = "default",
    role = "dialog",
    ariaDescribedById,
    titleId,
    className = "",
  } = props;

  const { closeAria } = useModalTranslations();
  const generatedTitleId = useId();
  const dialogTitleId = titleId ?? generatedTitleId;

  useModalEffects(open, onClose);

  if (typeof document === "undefined") return null;

  return createPortal(
    <AnimatePresence>
      {open && (
        <motion.div
          className="modal-wrapper"
          ref={ref}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          <div
            className="modal-backdrop"
            onClick={onClose}
            aria-hidden="true"
          />

          <motion.div
            className={`modal-container modal-container--${size} ${className}`}
            role={role}
            aria-modal="true"
            aria-labelledby={dialogTitleId}
            aria-describedby={ariaDescribedById}
            tabIndex={-1}
            initial={{ opacity: 0, y: 20, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 20, scale: 0.95 }}
            transition={{ type: "spring", bounce: 0.3, duration: 0.4 }}
          >
            <div className="modal-header">
              <h2 className="modal-header__title" id={dialogTitleId}>
                {title}
              </h2>
              <Button
                variant="danger"
                isIcon={true}
                onClick={onClose}
                aria-label={closeAria}
              >
                <X size={14} />
              </Button>
            </div>

            <div className="modal-body">{children}</div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>,
    document.body,
  );
});

Modal.displayName = "Modal";

export default Modal;

export { Modal };
