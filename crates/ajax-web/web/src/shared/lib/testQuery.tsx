import { type ReactElement, type ReactNode, useRef } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { render, renderHook, type RenderHookOptions, type RenderOptions } from "@testing-library/react-original";
import { createQueryClient } from "./queryClient";

export function QueryTestProvider({ children }: { children: ReactNode }) {
  const clientRef = useRef(createQueryClient());
  return <QueryClientProvider client={clientRef.current}>{children}</QueryClientProvider>;
}

export function renderWithQuery(ui: ReactElement, options?: Omit<RenderOptions, "wrapper">) {
  return render(ui, { wrapper: QueryTestProvider, ...options });
}

export function renderHookWithQuery<Result, Props>(
  hook: (initialProps: Props) => Result,
  options?: Omit<RenderHookOptions<Props>, "wrapper">,
) {
  return renderHook(hook, { wrapper: QueryTestProvider, ...options });
}
