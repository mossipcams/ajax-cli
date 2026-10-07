import { copyText } from "./clipboard";

export type LinkActivation = {
  url: string;
  clientX: number;
  clientY: number;
};

export type LinkClickEvent = {
  preventDefault?: () => void;
  clientX: number;
  clientY: number;
};

export type TerminalLinkService = {
  handleLinkClick: (event: LinkClickEvent, uri: string) => void;
  onOpen: (url: string) => void;
  onCopy: (url: string) => Promise<boolean>;
  subscribe: (listener: (activation: LinkActivation) => void) => () => void;
  dispose: () => void;
};

export function createTerminalLinkService(): TerminalLinkService {
  const listeners = new Set<(activation: LinkActivation) => void>();
  let disposed = false;

  const notify = (activation: LinkActivation) => {
    if (disposed) return;
    for (const listener of listeners) {
      listener(activation);
    }
  };

  return {
    handleLinkClick(event, uri) {
      if (disposed) return;
      event.preventDefault?.();
      notify({
        url: uri,
        clientX: event.clientX,
        clientY: event.clientY,
      });
    },
    onOpen(url) {
      let parsed: URL;
      try {
        parsed = new URL(url);
      } catch {
        return;
      }
      if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return;
      if (parsed.username || parsed.password) return;

      const safeHref = parsed.href;

      const anchor = document.createElement("a");
      anchor.href = safeHref;
      anchor.target = "_blank";
      anchor.rel = "noopener noreferrer";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
    },
    async onCopy(url) {
      return copyText(url);
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    dispose() {
      disposed = true;
      listeners.clear();
    },
  };
}
