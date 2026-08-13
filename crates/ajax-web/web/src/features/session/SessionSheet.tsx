import type { ReactNode } from "react";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Button } from "@/shared/ui/button";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";

interface Props {
  testId: string;
  label: string;
  title: string;
  className: string;
  onClose: () => void;
  children: ReactNode;
}

export default function SessionSheet({
  testId,
  label,
  title,
  className,
  onClose,
  children,
}: Props) {
  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={(open) => !open && onClose()}>
        <SheetContent asChild aria-describedby={undefined}>
          <div
            className="session-sheet-scrim"
            onPointerDown={(event) => {
              if (event.target === event.currentTarget) onClose();
            }}
          >
            <div
              className={className}
              data-testid={testId}
              role="dialog"
              aria-modal="true"
              aria-label={label}
            >
              <div className="session-sheet-header">
                <SheetTitle asChild>
                  <h2>{title}</h2>
                </SheetTitle>
                <Button type="button" variant="secondary" onClick={onClose}>
                  Close
                </Button>
              </div>
              {children}
            </div>
          </div>
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}
