export type TerminalSnapshotTarget = {
  reset: () => void;
  write: (data: string) => void;
};

export type TerminalSnapshot = {
  capture: () => void;
  restore: (term: TerminalSnapshotTarget) => boolean;
  clear: () => void;
  getSnapshot: () => string | undefined;
  dispose: () => void;
};

export function createTerminalSnapshot(serialize: () => string): TerminalSnapshot {
  let snapshot: string | undefined;
  let disposed = false;

  return {
    capture() {
      if (disposed) return;
      snapshot = serialize();
    },
    restore(term) {
      if (disposed || snapshot === undefined) return false;
      term.reset();
      term.write(snapshot);
      return true;
    },
    clear() {
      snapshot = undefined;
    },
    getSnapshot() {
      return snapshot;
    },
    dispose() {
      disposed = true;
      snapshot = undefined;
    },
  };
}
