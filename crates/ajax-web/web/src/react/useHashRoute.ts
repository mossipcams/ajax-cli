import { useEffect, useState } from "react";
import { parseRoute, type Route } from "../routes";

export function useHashRoute(): Route {
  const [route, setRoute] = useState(() => parseRoute(location.hash));

  useEffect(() => {
    const onHashChange = () => setRoute(parseRoute(location.hash));
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  return route;
}
