import { useEffect } from "react";
import { initViewport } from "../viewport";

export function useViewportBand(): void {
  useEffect(() => initViewport(), []);
}
