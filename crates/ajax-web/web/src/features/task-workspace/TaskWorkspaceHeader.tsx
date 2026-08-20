import { statusMeta } from "@/shared/lib/state";
import type { BrowserTaskDetail } from "@/shared/lib/types";

export interface TaskWorkspaceHeaderProps {
  detail?: BrowserTaskDetail | null;
  /** Shown as the title when detail has not loaded yet. */
  handle?: string;
  onBack: () => void;
  onOpenDetails?: () => void;
  detailsOpen?: boolean;
  detailsPanelId?: string;
  detailsTestId?: string;
}

/** Shared task identity row: back, title, details affordance, status pill. */
export default function TaskWorkspaceHeader({
  detail,
  handle,
  onBack,
  onOpenDetails,
  detailsOpen = false,
  detailsPanelId,
  detailsTestId = "task-details",
}: TaskWorkspaceHeaderProps) {
  const meta = detail ? statusMeta(detail.status) : null;
  const title = detail?.title || detail?.qualified_handle || handle || "";

  return (
    <div
      className="detail-header"
      data-mobile-chrome="header"
      data-testid="mobile-chrome-header"
    >
      <button type="button" className="back" onClick={onBack}>
        ← Back
      </button>
      <h1 className="detail-title">{title}</h1>
      <div className="detail-header-controls">
        {onOpenDetails ? (
          <button
            type="button"
            className="session-head-details"
            data-testid={detailsTestId}
            aria-expanded={detailsOpen}
            {...(detailsPanelId ? { "aria-controls": detailsPanelId } : {})}
            onClick={onOpenDetails}
          >
            Details
          </button>
        ) : null}
        {meta ? <span className={`interact-pill tone-${meta.tone}`}>{meta.label}</span> : null}
      </div>
    </div>
  );
}
