export type SwipeEnterDirection = "left" | "right";

const STORAGE_KEY = "ajax-swipe-enter";

export function setSwipeEnterDirection(direction: SwipeEnterDirection): void {
  try {
    sessionStorage.setItem(STORAGE_KEY, direction);
  } catch {
  }
}

export function navigateHashWithEnter(hash: string, direction: SwipeEnterDirection): void {
  setSwipeEnterDirection(direction);
  location.hash = hash;
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
