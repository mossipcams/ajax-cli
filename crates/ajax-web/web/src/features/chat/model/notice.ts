import { useEffect, useRef, useState } from "react";

export function useSessionModelNotice() {
  const [notice, setNotice] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  function showNotice(message: string) {
    setNotice(message);
  }

  function dismissNotice() {
    setNotice(null);
  }

  return { notice, showNotice, dismissNotice };
}

export function useSessionModelSheet() {
  const [modelSheetOpen, setModelSheetOpen] = useState(false);
  return { modelSheetOpen, setModelSheetOpen };
}
