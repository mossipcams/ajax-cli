import { useComposerContext } from "./useComposer";

export default function QueuedFollowUp() {
  const { queued, stopping, editQueued, removeQueued } = useComposerContext();
  if (queued === null) return null;

  return (
    <div className="session-queued" data-testid="session-queued">
      <p className="session-queued-label">{stopping ? "Stopping…" : "Queued"}</p>
      <article className="session-said is-queued">{queued}</article>
      {stopping ? null : (
        <p className="session-queued-hint">Press Enter again to stop and send now</p>
      )}
      <div className="session-queued-actions">
        <button type="button" onClick={editQueued}>
          Edit
        </button>
        <button type="button" onClick={removeQueued}>
          Remove
        </button>
      </div>
    </div>
  );
}
