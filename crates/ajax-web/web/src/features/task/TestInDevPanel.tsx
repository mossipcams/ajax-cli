import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@/shared/ui/button";
import { ApiError, startDevDeploy } from "@/shared/lib/api";
import { queryKeys } from "@/shared/lib/queryClient";
import { useDevDeployQuery } from "./useDevDeployQuery";

interface Props {
  taskHandle: string;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
}

export default function TestInDevPanel({ taskHandle, onResult }: Props) {
  const { data } = useDevDeployQuery();
  const queryClient = useQueryClient();
  const status = data?.deploy ?? null;
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  async function deploy() {
    if (busyRef.current || busy || status?.active) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const response = await startDevDeploy(taskHandle);
      queryClient.setQueryData(queryKeys.devDeploy(), response);
    } catch (error) {
      const message =
        error instanceof ApiError ? error.message : "Test in Dev failed to start";
      onResult?.(message, null, true);
      await queryClient.invalidateQueries({ queryKey: queryKeys.devDeploy() });
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  const phaseLabel = status?.phase_label ?? "Ready to deploy";
  const disabled = busy || !!status?.active;
  const error = status?.error ?? null;

  return (
    <section className="test-in-dev" data-testid="test-in-dev" aria-label="Test in Dev">
      <div className="test-in-dev-row">
        <div className="actions">
          <Button
            type="button"
            variant="secondary"
            data-testid="test-in-dev-button"
            disabled={disabled}
            onClick={() => void deploy()}
          >
            {disabled ? `${phaseLabel}…` : "Test in Dev"}
          </Button>
        </div>
      </div>
      {error ? (
        <pre className="error" data-testid="test-in-dev-error">
          {error}
        </pre>
      ) : null}
    </section>
  );
}
