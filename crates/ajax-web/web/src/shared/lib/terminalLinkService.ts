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
      window.open(url, "_blank", "noopener,noreferrer");
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
