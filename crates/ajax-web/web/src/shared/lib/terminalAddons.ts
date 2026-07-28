import type { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SerializeAddon } from "@xterm/addon-serialize";
import {
  createTerminalLinkService,
  type LinkActivation,
  type TerminalLinkService,
} from "./terminalLinkService";
import { createTerminalSnapshot, type TerminalSnapshot } from "./terminalSnapshot";

const HTTP_URL_REGEX = /(https?:\/\/[^\s"'<>]+)/i;

export type AttachTerminalAddonsOptions = {
  onLinkActivate: (activation: LinkActivation) => void;
};

export type TerminalAddons = {
  fitAddon: FitAddon;
  linkService: TerminalLinkService;
  snapshot: TerminalSnapshot;
  dispose: () => void;
};

export function attachTerminalAddons(
  term: Terminal,
  options: AttachTerminalAddonsOptions,
): TerminalAddons {
  const fitAddon = new FitAddon();
  const serializeAddon = new SerializeAddon();
  const linkService = createTerminalLinkService();
  const unsubscribe = linkService.subscribe(options.onLinkActivate);
  const snapshot = createTerminalSnapshot(() => serializeAddon.serialize());

  const webLinksAddon = new WebLinksAddon(
    (event, uri) => {
      linkService.handleLinkClick(event, uri);
    },
    { urlRegex: HTTP_URL_REGEX },
  );

  term.loadAddon(fitAddon);
  term.loadAddon(webLinksAddon);
  term.loadAddon(serializeAddon);

  return {
    fitAddon,
    linkService,
    snapshot,
    dispose() {
      unsubscribe();
      linkService.dispose();
      fitAddon.dispose();
      webLinksAddon.dispose();
      serializeAddon.dispose();
      snapshot.dispose();
    },
  };
}
