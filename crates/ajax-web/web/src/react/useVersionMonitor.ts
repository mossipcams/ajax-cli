import { useCallback, useRef, useState } from "react";
import { fetchVersion } from "../api";

export type VersionMonitor = {
  updateAvailable: boolean;
  checkVersion: () => Promise<void>;
};

export function useVersionMonitor(): VersionMonitor {
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const bootVersionRef = useRef<string | null>(null);

  const checkVersion = useCallback(async () => {
    try {
      const { version } = await fetchVersion();
      if (!version) return;
      if (bootVersionRef.current === null) bootVersionRef.current = version;
      else if (version !== bootVersionRef.current) setUpdateAvailable(true);
    } catch {
      // Offline: keep the pinned version and retry later.
    }
  }, []);

  return { updateAvailable, checkVersion };
}
