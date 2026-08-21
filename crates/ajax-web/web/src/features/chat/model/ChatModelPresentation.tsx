import type { ReactNode } from "react";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import SessionModelControls, {
  SessionModelNotice,
  SessionModelOpenButton,
  hasSessionModelControls,
  sessionModelControlLabel,
} from "./SessionModelControls";

interface Props {
  handle: string;
  agent: string | undefined;
  connected: boolean;
  confirmedModel: string;
  configOptions: LiveSessionConfigOption[] | undefined;
  notice: string | null;
  dismissNotice: () => void;
  modelSheetOpen: boolean;
  setModelSheetOpen: (open: boolean) => void;
  onApply: (configId: string, value: string | boolean) => void;
  renderComposer: (slots: { notice: ReactNode; modelControl: ReactNode }) => ReactNode;
}

export default function ChatModelPresentation({
  handle,
  agent,
  connected,
  confirmedModel,
  configOptions,
  notice,
  dismissNotice,
  modelSheetOpen,
  setModelSheetOpen,
  onApply,
  renderComposer,
}: Props) {
  const modelPanelId = `session-model-${handle}`;
  const showModelControl =
    Boolean(configOptions?.length) && hasSessionModelControls(agent, configOptions ?? []);
  const modelButtonLabel = sessionModelControlLabel(confirmedModel, configOptions);

  const noticeSlot = notice ? (
    <SessionModelNotice message={notice} onDismiss={dismissNotice} />
  ) : null;
  const modelControlSlot = showModelControl ? (
    <SessionModelOpenButton
      panelId={modelPanelId}
      label={modelButtonLabel}
      disabled={!connected}
      expanded={modelSheetOpen}
      onOpen={() => setModelSheetOpen(true)}
    />
  ) : null;

  return (
    <>
      {renderComposer({ notice: noticeSlot, modelControl: modelControlSlot })}
      {showModelControl ? (
        <SessionModelControls
          open={modelSheetOpen}
          onOpenChange={setModelSheetOpen}
          panelId={modelPanelId}
          agent={agent}
          confirmedModel={confirmedModel}
          options={configOptions!}
          disabled={!connected}
          onApply={onApply}
        />
      ) : null}
    </>
  );
}
