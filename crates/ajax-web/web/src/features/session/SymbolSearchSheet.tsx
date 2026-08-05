import { useEffect, useRef, useState } from "react";
import { fetchTaskSymbols } from "@/shared/lib/api";
import { useSheetDrag } from "@/shared/hooks/useSheetDrag";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Button } from "@/shared/ui/button";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import type { WebSessionSymbolContext } from "./types";

interface Props {
  handle: string;
  open: boolean;
  selected: WebSessionSymbolContext[];
  onClose: () => void;
  onConfirm: (symbols: WebSessionSymbolContext[]) => void;
}

export default function SymbolSearchSheet({
  handle,
  open,
  selected,
  onClose,
  onConfirm,
}: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<WebSessionSymbolContext[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draftSelected, setDraftSelected] = useState<WebSessionSymbolContext[]>(selected);
  const [dragOffset, setDragOffset] = useState(0);
  const sheetRef = useRef<HTMLDivElement>(null);
  const grabRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setDraftSelected(selected);
    setQuery("");
    setResults([]);
    setError(null);
    sheetRef.current?.focus();
  }, [open, selected]);

  useSheetDrag(grabRef, {
    onDismiss: onClose,
    onOffset: setDragOffset,
  });

  useEffect(() => {
    if (!open) return;
    const trimmed = query.trim();
    if (!trimmed) {
      setResults([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    const timer = window.setTimeout(() => {
      fetchTaskSymbols(handle, trimmed)
        .then((symbols) => {
          setResults(symbols);
          setLoading(false);
        })
        .catch((cause: Error) => {
          setError(cause.message);
          setResults([]);
          setLoading(false);
        });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [handle, open, query]);

  function toggleSymbol(symbol: WebSessionSymbolContext) {
    setDraftSelected((prev) => {
      if (prev.some((item) => item.id === symbol.id)) {
        return prev.filter((item) => item.id !== symbol.id);
      }
      return prev.concat(symbol);
    });
  }

  if (!open) return null;

  return (
    <Sheet open onOpenChange={(next) => !next && onClose()}>
      <FullscreenLayer>
        <SheetContent className="symbol-search-sheet" data-testid="symbol-search-sheet">
          <div
            id="symbol-search-sheet"
            data-testid="symbol-search-sheet-card"
            className={`sheet-card${dragOffset > 0 ? " is-dragging" : ""}`}
            ref={sheetRef}
            tabIndex={-1}
          >
            <div
              className="sheet-grab"
              ref={grabRef}
              data-testid="symbol-search-sheet-grab"
              aria-hidden="true"
            >
              <span className="sheet-grabber" />
            </div>

            <SheetTitle className="symbol-search-sheet-title">Add context</SheetTitle>

            <label className="field-label" htmlFor="symbol-search-input">
              Search symbols
            </label>
            <input
              id="symbol-search-input"
              data-testid="symbol-search-input"
              className="symbol-search-input"
              value={query}
              placeholder="Function, struct, file…"
              onChange={(event) => setQuery(event.target.value)}
            />

            {error ? (
              <p className="sheet-error" role="alert" data-testid="symbol-search-error">
                {error}
              </p>
            ) : null}

            <div className="symbol-search-results" data-testid="symbol-search-results">
              {loading ? <p className="symbol-search-hint">Searching…</p> : null}
              {!loading && query.trim() && results.length === 0 ? (
                <p className="symbol-search-hint">No matches</p>
              ) : null}
              {results.map((symbol) => {
                const checked = draftSelected.some((item) => item.id === symbol.id);
                return (
                  <button
                    key={symbol.id}
                    type="button"
                    className={`symbol-search-row${checked ? " is-selected" : ""}`}
                    data-testid={`symbol-search-row-${symbol.id}`}
                    onClick={() => toggleSymbol(symbol)}
                  >
                    <span className="symbol-search-row-name">{symbol.name}</span>
                    <span className="symbol-search-row-meta">
                      {symbol.kind} · {symbol.path}
                    </span>
                  </button>
                );
              })}
            </div>

            <div className="sheet-actions">
              <Button type="button" variant="secondary" onClick={onClose}>
                Cancel
              </Button>
              <Button
                type="button"
                data-testid="symbol-search-confirm"
                onClick={() => onConfirm(draftSelected)}
              >
                Add {draftSelected.length > 0 ? `(${draftSelected.length})` : ""}
              </Button>
            </div>
          </div>
        </SheetContent>
      </FullscreenLayer>
    </Sheet>
  );
}
