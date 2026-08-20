import { useEffect, useMemo, useRef } from "react";
import { Button } from "@/shared/ui/button";
import {
  DEFAULT_SESSION_MODEL,
  catalogIdToStoragePipe,
  defaultCatalogIdForCursorGroup,
  groupCursorCatalogModels,
  modelChoicesFromOptionCatalog,
  normalizeSessionAgent,
  resolveCursorCatalogSelection,
  type SessionModelCatalog,
} from "./desiredModel";
import { useOptionCatalogQuery } from "./useOptionCatalogQuery";
import { useSessionModelsQuery } from "./useSessionModelsQuery";

interface Props {
  /** Harness whose advertised options to list. */
  agent: string;
  agentLabel: string;
  /** Advertised model id, or legacy pipe-form / exploded catalog id. */
  value: string;
  disabled?: boolean;
  onChange: (selection: string) => void;
  /** Called once with the harness default so callers can preselect it. */
  onCatalog?: (catalog: SessionModelCatalog) => void;
}

/**
 * New Task / idle Switch picker.
 * Cursor lists exploded ids from GET /api/session/models; onChange emits pipe storage.
 * Other harnesses list last-advertised option-catalog model ids.
 * Connected sessions use advertised configOptions via set_config_option.
 */
export default function ModelPicker({
  agent,
  agentLabel,
  value,
  disabled = false,
  onChange,
  onCatalog,
}: Props) {
  const harness = normalizeSessionAgent(agent);
  const isCursor = harness === "cursor";
  const sessionQuery = useSessionModelsQuery(agent, { enabled: isCursor });
  const catalogQuery = useOptionCatalogQuery(agent, { enabled: !isCursor });
  const listRef = useRef<HTMLDivElement>(null);
  const catalogNotifiedRef = useRef<string | null>(null);

  const isPending = isCursor ? sessionQuery.isPending : catalogQuery.isPending;
  const isError = isCursor ? sessionQuery.isError : catalogQuery.isError;
  const refetch = isCursor ? sessionQuery.refetch : catalogQuery.refetch;

  const catalog = useMemo((): SessionModelCatalog | undefined => {
    if (isCursor) {
      if (sessionQuery.data === undefined) return undefined;
      return sessionQuery.data;
    }
    if (catalogQuery.data === undefined) return undefined;
    const { models, default: defaultModel } = modelChoicesFromOptionCatalog(catalogQuery.data);
    return {
      models,
      default: defaultModel,
      ...(catalogQuery.data.error ? { error: catalogQuery.data.error } : {}),
    };
  }, [isCursor, sessionQuery.data, catalogQuery.data]);

  const catalogIds = useMemo(
    () => (catalog ? catalog.models.map((option) => option.id) : []),
    [catalog],
  );

  const selectedId = useMemo(() => {
    if (isCursor) return resolveCursorCatalogSelection(value, catalogIds);
    const trimmed = value.trim();
    return trimmed;
  }, [isCursor, value, catalogIds]);

  const unknownModel =
    Boolean(selectedId) &&
    selectedId !== DEFAULT_SESSION_MODEL &&
    !catalogIds.includes(selectedId);

  useEffect(() => {
    if (!catalog) return;
    if (catalogNotifiedRef.current === agent) return;
    catalogNotifiedRef.current = agent;
    onCatalog?.({
      models: catalog.models,
      default: catalog.default,
      ...(catalog.error ? { error: catalog.error } : {}),
    });
  }, [agent, catalog, onCatalog]);

  useEffect(() => {
    if (!selectedId) return;
    listRef.current
      ?.querySelector<HTMLElement>("[aria-checked='true']")
      ?.scrollIntoView?.({ block: "nearest" });
  }, [selectedId, catalog]);

  if (isError) {
    return (
      <p className="sheet-error" data-testid="model-catalog-error">
        Could not read models from {agentLabel}.{" "}
        <Button type="button" variant="secondary" onClick={() => void refetch()}>
          Retry
        </Button>
      </p>
    );
  }

  if (isPending || catalog === undefined) {
    return <p className="sheet-note">Reading models from {agentLabel}…</p>;
  }

  const { models, default: defaultModel } = catalog;

  if (catalog.error && models.length === 0) {
    return (
      <p className="sheet-error" data-testid="model-catalog-error">
        {catalog.error}{" "}
        <Button type="button" variant="secondary" onClick={() => void refetch()}>
          Retry
        </Button>
      </p>
    );
  }

  if (models.length === 0) {
    return (
      <p className="sheet-note">
        {agentLabel} lists no models here; it will start on its own default.
      </p>
    );
  }

  if (isCursor) {
    const { auto, groups } = groupCursorCatalogModels(models);
    return (
      <div
        className="model-picker"
        role="radiogroup"
        aria-label={`${agentLabel} models`}
        ref={listRef}
      >
        {unknownModel ? (
          <button
            key={`unknown-${selectedId}`}
            type="button"
            className="model-option is-selected"
            role="radio"
            aria-checked
            disabled={disabled}
            onClick={() => onChange(value)}
          >
            <span className="model-option-label">{selectedId}</span>
          </button>
        ) : null}

        {auto.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`model-option${selectedId === option.id ? " is-selected" : ""}`}
            role="radio"
            aria-checked={selectedId === option.id}
            disabled={disabled}
            onClick={() => onChange(option.id)}
          >
            <span className="model-option-label">{option.label}</span>
            {option.id === defaultModel ? (
              <span className="model-option-tag">Default</span>
            ) : null}
          </button>
        ))}

        {groups.map((group) => {
          const headerId = `model-group-${group.base}`;
          return (
            <div
              key={group.base}
              className="model-group"
              role="group"
              aria-labelledby={headerId}
            >
              <button
                type="button"
                className="model-group-label"
                id={headerId}
                disabled={disabled}
                onClick={() =>
                  onChange(
                    catalogIdToStoragePipe(
                      defaultCatalogIdForCursorGroup(group, defaultModel, catalogIds),
                      catalogIds,
                    ),
                  )
                }
              >
                {group.label}
              </button>
              {group.variants.map((variant) => (
                <button
                  key={variant.id}
                  type="button"
                  className={`model-option${selectedId === variant.id ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={selectedId === variant.id}
                  disabled={disabled}
                  onClick={() => onChange(catalogIdToStoragePipe(variant.id, catalogIds))}
                >
                  <span className="model-option-label">{variant.label}</span>
                  {variant.id === defaultModel ? (
                    <span className="model-option-tag">Default</span>
                  ) : null}
                </button>
              ))}
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div
      className="model-picker"
      role="radiogroup"
      aria-label={`${agentLabel} models`}
      ref={listRef}
    >
      {unknownModel ? (
        <button
          key={`unknown-${selectedId}`}
          type="button"
          className="model-option is-selected"
          role="radio"
          aria-checked
          disabled={disabled}
          onClick={() => onChange(value)}
        >
          <span className="model-option-label">{selectedId}</span>
        </button>
      ) : null}

      {models.map((option) => (
        <button
          key={option.id}
          type="button"
          className={`model-option${selectedId === option.id ? " is-selected" : ""}`}
          role="radio"
          aria-checked={selectedId === option.id}
          disabled={disabled}
          onClick={() => onChange(option.id)}
        >
          <span className="model-option-label">{option.label}</span>
          {option.id === defaultModel ? (
            <span className="model-option-tag">Default</span>
          ) : null}
        </button>
      ))}
    </div>
  );
}
