import { useCallback, useRef, useState } from "react";
import { useFetchVersion } from "./useVersionQuery";

export type VersionMonitor = {
  updateAvailable: boolean;
  checkVersion: () => Promise<boolean>;
};

const APP_VERSION_PLACEHOLDER = "__AJAX_APP_VERSION__";

function loadedAppVersion(): string | null {
  const content = document.querySelector<HTMLMetaElement>('meta[name="ajax-app-version"]')?.content;
  return content && content !== APP_VERSION_PLACEHOLDER ? content : null;
}

export function useVersionMonitor(): VersionMonitor {
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const bootVersionRef = useRef<string | null>(null);
  const fetchVersion = useFetchVersion();

  const checkVersion = useCallback(async () => {
    try {
      const { version } = await fetchVersion();
      if (!version) return false;
      if (bootVersionRef.current === null) bootVersionRef.current = loadedAppVersion() ?? version;
      const available = version !== bootVersionRef.current;
      setUpdateAvailable(available);
      return available;
    } catch {
      return false;
    }
  }, [fetchVersion]);

  return { updateAvailable, checkVersion };
}
