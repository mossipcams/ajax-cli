import type { MouseEvent } from "react";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import { modelLiveOption } from "@/shared/lib/liveSessionConfig";
import { DEFAULT_SESSION_MODEL } from "@/features/task/desiredModel";
import ConfigPickers from "./ConfigPickers";

export interface ModelSwitchSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  panelId: string;
  agent?: string;
  confirmedModel: string;
  options: LiveSessionConfigOption[];
  disabled?: boolean;
  onApply: (configId: string, value: string | boolean) => void;
}

/** Short label for the hotbar model control when a friendly name is unavailable. */
export function modelControlLabel(
  confirmedModel: string,
  options?: LiveSessionConfigOption[],
): string {
  const trimmed = confirmedModel.trim();
  if (!trimmed || trimmed === DEFAULT_SESSION_MODEL) return "Auto";
  const model = modelLiveOption(options ?? []);
  const fromOption = model?.choices.find((choice) => choice.value === trimmed)?.name;
  if (fromOption) return fromOption;
  const tail = trimmed.replace(/^cursor-/, "").replace(/-/g, " ");
  return tail.length > 16 ? `${tail.slice(0, 14)}…` : tail;
}

export default function ModelSwitchSheet({
  open,
  onOpenChange,
  panelId,
  agent,
  confirmedModel,
  options,
  disabled = false,
  onApply,
}: ModelSwitchSheetProps) {
  if (!open) return null;

  function close() {
    onOpenChange(false);
  }

  function handleBackdropClick(event: MouseEvent<HTMLDivElement>) {
    if (event.target === event.currentTarget) close();
  }

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={(next) => !next && close()}>
        <SheetContent
          asChild
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            event.preventDefault();
          }}
        >
          {/* Backdrop dismissal only — same contract as NewTaskSheet. */}
          {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- backdrop click; Escape is Radix */}
          <div className="session-sheet-scrim" onClick={handleBackdropClick}>
            <div
              className="session-model-switch-sheet"
              id={panelId}
              data-testid="model-switch-sheet"
              role="dialog"
              aria-modal="true"
              aria-label="Choose model"
            >
              <div className="session-sheet-header">
                <SheetTitle asChild>
                  <h2>Model</h2>
                </SheetTitle>
                <Button
                  type="button"
                  variant="secondary"
                  className="session-sheet-close"
                  data-testid="model-switch-close"
                  onClick={close}
                >
                  Close
                </Button>
              </div>

              <div className="session-model-switch-body" data-testid="model-switch-body">
                <p className="session-model-switch-current" data-testid="model-switch-current">
                  Current: {modelControlLabel(confirmedModel, options)}
                </p>
                <ConfigPickers
                  agent={agent}
                  confirmedModel={confirmedModel}
                  options={options}
                  disabled={disabled}
                  onApply={onApply}
                />
              </div>
            </div>
          </div>
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}
