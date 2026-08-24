"use client";

import { Button, Modal } from "@heroui/react";
import type { ReactNode } from "react";

interface ConfirmModalProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
}

export function ConfirmModal({
  isOpen,
  onOpenChange,
  title,
  description,
  confirmLabel = "确定",
  cancelLabel = "取消",
  danger = false,
  onConfirm,
}: ConfirmModalProps) {
  return (
    <Modal isOpen={isOpen} onOpenChange={onOpenChange}>
      <Modal.Backdrop>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <div className="text-sm text-white/70">{description}</div>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="tertiary" onPress={() => onOpenChange(false)}>
                {cancelLabel}
              </Button>
              <Button
                variant={danger ? "danger" : "primary"}
                onPress={async () => {
                  await onConfirm();
                  onOpenChange(false);
                }}
              >
                {confirmLabel}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}