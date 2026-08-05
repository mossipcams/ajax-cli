import { useEffect, useRef, useState } from "react";
import { useSheetDrag } from "@/shared/hooks/useSheetDrag";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Button } from "@/shared/ui/button";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import type { WebSessionSymbolContext } from "./types";

interface Props {
  symbol: WebSessionSymbolContext | null;
  open: boolean;
  attached: WebSessionSymbolContext[];
  onClose: () => void;
  onAttach: (symbol: WebSessionSymbolContext) => void;
}

export default function SymbolDetailSheet({
  symbol,
  open,
  attached,
  onClose,
  onAttach,
}: Props) {
  const [dragOffset, setDragOffset] = useState(0);
  const sheetRef = useRef<HTMLDivElement>(null);
  const grabRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    sheetRef.current?.focus();
  }, [open, symbol?.id]);

  useSheetDrag(grabRef, {
    onDismiss: onClose,
    onOffset: setDragOffset,
  });

  if (!open || !symbol) return null;

  const alreadyAttached = attached.some((item) => item.id === symbol.id);

  return (
    <Sheet open onOpenChange={(next) => !next && onClose()}>
      <FullscreenLayer>
        <SheetContent className="symbol-detail-sheet" data-testid="symbol-detail-sheet">
          <div
            data-testid="symbol-detail-sheet-card"
            className={`sheet-card${dragOffset > 0 ? " is-dragging" : ""}`}
            ref={sheetRef}
            tabIndex={-1}
          >
            <div
              className="sheet-grab"
              ref={grabRef}
              data-testid="symbol-detail-sheet-grab"
              aria-hidden="true"
            >
              <span className="sheet-grabber" />
            </div>

            <SheetTitle className="symbol-detail-sheet-title">{symbol.name}</SheetTitle>

            <p className="symbol-detail-meta" data-testid="symbol-detail-meta">
              {symbol.kind} · {symbol.path}
            </p>

            <pre className="symbol-detail-source" data-testid="symbol-detail-source">
              {symbol.source}
            </pre>

            <div className="sheet-actions">
              <Button type="button" variant="secondary" onClick={onClose}>
                Close
              </Button>
              <Button
                type="button"
                data-testid="symbol-detail-attach"
                disabled={alreadyAttached}
                onClick={() => onAttach(symbol)}
              >
                {alreadyAttached ? "Attached" : "Attach to next message"}
              </Button>
            </div>
          </div>
        </SheetContent>
      </FullscreenLayer>
    </Sheet>
  );
}
