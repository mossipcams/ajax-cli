export type SwipeEnterDirection = "left" | "right";

const STORAGE_KEY = "ajax-swipe-enter";

export function setSwipeEnterDirection(direction: SwipeEnterDirection): void {
  try {
    sessionStorage.setItem(STORAGE_KEY, direction);
  } catch {
  }
}

export function navigateHash(hash: string): void {
  const previous = location.hash;
  if (previous === hash) return;
  location.hash = hash;
  history.replaceState({ ...(history.state ?? {}), ajaxPrevHash: previous }, "");
}

export function navigateHashWithEnter(hash: string, direction: SwipeEnterDirection): void {
  setSwipeEnterDirection(direction);
  navigateHash(hash);
}

let pendingHistoryBack: (() => void)[] | null = null;

export function afterPendingHistoryBack(callback: () => void): void {
  if (pendingHistoryBack) pendingHistoryBack.push(callback);
  else callback();
}

export function navigateHashBack(hash: string): void {
  setSwipeEnterDirection("right");
  if (history.state?.ajaxPrevHash === hash) {
    const fromHash = location.hash;
    const callbacks: (() => void)[] = [];
    pendingHistoryBack = callbacks;
    const onHashChange = () => {
      if (location.hash === fromHash) return;
      window.removeEventListener("hashchange", onHashChange);
      if (pendingHistoryBack === callbacks) pendingHistoryBack = null;
      callbacks.forEach((callback) => callback());
    };
    window.addEventListener("hashchange", onHashChange);
    history.back();
    return;
  }
  location.replace(`${location.pathname}${location.search}${hash}`);
}

export function consumeSwipeEnterDirection(): SwipeEnterDirection | null {
  try {
    const value = sessionStorage.getItem(STORAGE_KEY);
    if (value === "left" || value === "right") {
      sessionStorage.removeItem(STORAGE_KEY);
      return value;
    }
  } catch {
  }
  return null;
}

export function swipeEnterClassName(direction: SwipeEnterDirection | null): string {
  return direction ? `ajax-swipe-enter-${direction}` : "";
}
