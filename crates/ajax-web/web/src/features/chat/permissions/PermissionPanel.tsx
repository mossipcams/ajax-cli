import { Button } from "@/shared/ui/button";
import type { Decision } from "../session/public";

interface Props {
  decision: Decision;
  connected: boolean;
  onApprove: () => void;
  onReject: () => void;
}

export default function PermissionPanel({ decision, connected, onApprove, onReject }: Props) {
  return (
    <div className="session-decision" data-testid="session-decision" role="alert">
      <p className="session-decision-title">{decision.title}</p>
      {decision.detail ? <p className="session-decision-detail">{decision.detail}</p> : null}
      {}
      <div className="session-decision-actions">
        <Button type="button" variant="default" disabled={!connected} onClick={onApprove}>
          Approve
        </Button>
        <Button type="button" variant="secondary" disabled={!connected} onClick={onReject}>
          Reject
        </Button>
      </div>
    </div>
  );
}
