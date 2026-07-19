import { useEffect } from "react";
import { initViewport } from "@/shared/lib/viewport";

export function useViewportBand(): void {
  useEffect(() => initViewport(), []);
}
