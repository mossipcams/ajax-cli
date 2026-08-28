import {
  checkHealth,
  fetchRuntimeStatus,
  waitForServerRestart,
} from "@/shared/lib/api";
import {
  RUNTIME_OPERATION_TIMEOUT_MS,
  RUNTIME_STATUS_POLL_MS,
} from "@/shared/lib/polling";
import type { RuntimeOperationResult, RuntimeStatusResponse } from "@/shared/lib/types";

const TERMINAL_RESULTS: RuntimeOperationResult[] = ["succeeded", "failed", "rolled_back"];

export function isTerminalRuntimeResult(
  status: RuntimeStatusResponse | null | undefined,
): RuntimeOperationResult | null {
  const result = status?.operation?.result;
  if (result && TERMINAL_RESULTS.includes(result)) {
    return result;
  }
  const phase = status?.operation?.phase;
  if (phase && TERMINAL_RESULTS.includes(phase as RuntimeOperationResult)) {
    return phase as RuntimeOperationResult;
  }
  return null;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForRuntimeOperationResult(options?: {
  timeoutMs?: number;
  pollMs?: number;
  previousVersion?: string | null;
  /** When false (remote stable deploy from dev), skip health down-edge wait. */
  restarting?: boolean;
  onStatus?: (status: RuntimeStatusResponse) => void;
}): Promise<{ online: boolean; result: RuntimeOperationResult | null; status: RuntimeStatusResponse | null }> {
  const timeoutMs = options?.timeoutMs ?? RUNTIME_OPERATION_TIMEOUT_MS;
  const pollMs = options?.pollMs ?? RUNTIME_STATUS_POLL_MS;
  const restarting = options?.restarting ?? true;
  let last: RuntimeStatusResponse | null = null;

  const pollOnce = async (): Promise<RuntimeOperationResult | null> => {
    try {
      last = await fetchRuntimeStatus();
      options?.onStatus?.(last);
      return isTerminalRuntimeResult(last);
    } catch {
      return null;
    }
  };

  if (!restarting) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const terminal = await pollOnce();
      if (terminal) {
        return { online: true, result: terminal, status: last };
      }
      await sleep(pollMs);
    }
    return { online: true, result: isTerminalRuntimeResult(last), status: last };
  }

  const healthPromise = waitForServerRestart({
    timeoutMs,
    pollMs,
    previousVersion: options?.previousVersion ?? null,
  });
  const deadline = Date.now() + timeoutMs;
  let earlyTerminal: RuntimeOperationResult | null = null;

  while (Date.now() < deadline) {
    const terminal = await pollOnce();
    if (terminal === "failed" || terminal === "rolled_back") {
      earlyTerminal = terminal;
      break;
    }

    const healthFinished = await Promise.race([
      healthPromise.then(() => true),
      sleep(pollMs).then(() => false),
    ]);
    if (healthFinished) {
      break;
    }

    await sleep(pollMs);
  }

  if (earlyTerminal) {
    return { online: await checkHealth(), result: earlyTerminal, status: last };
  }

  const online = await healthPromise;
  if (!online) {
    return { online: false, result: isTerminalRuntimeResult(last), status: last };
  }

  const settleDeadline = Date.now() + 30_000;
  while (Date.now() < settleDeadline) {
    const terminal = await pollOnce();
    if (terminal) {
      return { online: true, result: terminal, status: last };
    }
    await sleep(pollMs);
  }

  return { online: true, result: isTerminalRuntimeResult(last), status: last };
}
