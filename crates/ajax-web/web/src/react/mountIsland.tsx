import { createElement, type ComponentType } from "react";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { ErrorBoundary } from "./ErrorBoundary";

export interface IslandHandle<P extends object> {
  update(nextProps: P): void;
  unmount(): void;
}

export function mountIsland<P extends object>(
  target: Element,
  Component: ComponentType<P>,
  props: P,
): IslandHandle<P> {
  let currentProps = props;
  const root: Root = createRoot(target);

  function render(): void {
    flushSync(() => {
      root.render(
        createElement(
          ErrorBoundary,
          null,
          createElement(Component, currentProps),
        ),
      );
    });
  }

  render();

  return {
    update(nextProps: P): void {
      currentProps = nextProps;
      render();
    },
    unmount(): void {
      root.unmount();
    },
  };
}
