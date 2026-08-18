import { useCallback, useRef, useState } from "react";
import { useFetchVersion } from "./useVersionQuery";

export type VersionMonitor = {
  updateAvailable: boolean;
  checkVersion: () => Promise<void>;
};

export function useVersionMonitor(): VersionMonitor {
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const bootVersionRef = useRef<string | null>(null);
  const fetchVersion = useFetchVersion();

  const checkVersion = useCallback(async () => {
    try {
      const { version } = await fetchVersion();
      if (!version) return;
      if (bootVersionRef.current === null) bootVersionRef.current = version;
      else setUpdateAvailable(version !== bootVersionRef.current);
    } catch {
      // Offline: keep the pinned version and retry later.
    }
  }, [fetchVersion]);

  return { updateAvailable, checkVersion };
}
