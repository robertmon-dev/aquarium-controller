import { useT } from "@aquarium/i18n/use-locale";
import { AlertCircle, Info, RotateCcw } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/atoms/Button/Button";
import { Modal } from "@/components/molecules/Modal/Modal";
import type { ConfirmDialogProps } from "./types";
import "./ConfirmDialog.scss";

export const ConfirmDialog = ({
  open,
  title,
  message,
  confirmText,
  cancelText,
  onConfirm,
  onCancel,
  isLoading,
  variant = "success",
}: ConfirmDialogProps) => {
  const t = useT();

  const Icon = useMemo(() => {
    switch (variant) {
      case "danger":
        return (
          <AlertCircle size={48} className="confirm-dialog__icon--danger" />
        );
      case "success":
        return (
          <RotateCcw size={48} className="confirm-dialog__icon--success" />
        );
      default:
        return <Info size={48} className="confirm-dialog__icon--primary" />;
    }
  }, [variant]);

  return (
    <Modal
      open={open}
      title={title}
      onClose={onCancel}
      role="alertdialog"
      size="40vw"
    >
      <div className="confirm-dialog">
        <div className="confirm-dialog__header">{Icon}</div>

        <p className="confirm-dialog__message">{message}</p>

        <div className="confirm-dialog__actions">
          <Button
            variant="ghost"
            onClick={onCancel}
            disabled={isLoading ?? false}
            fullWidth
          >
            {cancelText || t("common.cancel")}
          </Button>
          <Button
            variant={variant}
            onClick={onConfirm}
            isLoading={isLoading ?? false}
            fullWidth
          >
            {confirmText}
          </Button>
        </div>
      </div>
    </Modal>
  );
};
