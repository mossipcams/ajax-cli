import { agentLabel } from "@/features/task/agents";
import ModelPicker from "./ModelPicker";

interface Props {
  id: string;
  /** Composite selection from the host, e.g. `opus|effort=high`. */
  value: string;
  disabled?: boolean;
  /** Harness whose catalog to list. */
  agent?: string;
  onChange: (model: string) => void;
}

/** In-session model control for task details — button list, not a native select. */
export default function SessionModelSelect({ id, value, disabled, agent = "cursor", onChange }: Props) {
  return (
    <div className="session-model-picker" data-testid="session-model-select">
      <span className="field-label" id={id}>
        Model
      </span>
      <ModelPicker
        agent={agent}
        agentLabel={agentLabel(agent)}
        value={value}
        disabled={disabled}
        onChange={onChange}
      />
    </div>
  );
}
