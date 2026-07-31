import type { ActiveStatus, FleetSegment } from "@/shared/lib/state";
import { statusMeta } from "@/shared/lib/state";

interface Props {
  segments: FleetSegment[];
  selected: ActiveStatus | null;
  onSelect: (status: ActiveStatus | null) => void;
}

// The active fleet as one proportional, tone-segmented gauge — the first-second
// answer to "is the fleet healthy". Not a metric strip: no cards, no hero
// numbers, and a state with no tasks simply has no segment. Tapping a segment
// filters the tiers below to that state; tapping the live one clears.
export default function MusterBar({ segments, selected, onSelect }: Props) {
  if (segments.length === 0) return null;

  return (
    <div className="muster-bar" role="group" aria-label="Fleet status">
      {segments.map(({ status, count }) => {
        const { label } = statusMeta(status);
        const isSelected = selected === status;
        return (
          <button
            key={status}
            type="button"
            className={`muster-seg tone-${status}`}
            style={{ flexGrow: count }}
            data-status={status}
            aria-pressed={isSelected}
            aria-label={
              isSelected
                ? `Showing ${count} ${label} — tap to clear filter`
                : `${count} ${label} — tap to filter`
            }
            onClick={() => onSelect(isSelected ? null : status)}
          >
            <span className="muster-seg-dot" aria-hidden="true" />
            <span className="muster-seg-count">{count}</span>
            <span className="muster-seg-label">{label}</span>
          </button>
        );
      })}
    </div>
  );
}
