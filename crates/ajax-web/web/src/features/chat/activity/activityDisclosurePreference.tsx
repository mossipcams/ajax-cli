import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from "react";

type ActivityDisclosurePreferenceContextValue = {
  preference: boolean | null;
  setPreference: (expanded: boolean) => void;
};

const ActivityDisclosurePreferenceContext =
  createContext<ActivityDisclosurePreferenceContextValue | null>(null);

const noopPreference: ActivityDisclosurePreferenceContextValue = {
  preference: null,
  setPreference: () => {},
};

/** Session-scoped expand/collapse default for turn activity disclosures. */
export function ActivityDisclosurePreferenceProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<boolean | null>(null);
  const setPreference = useCallback((expanded: boolean) => {
    setPreferenceState(expanded);
  }, []);

  return (
    <ActivityDisclosurePreferenceContext.Provider value={{ preference, setPreference }}>
      {children}
    </ActivityDisclosurePreferenceContext.Provider>
  );
}

export function useActivityDisclosurePreference(): ActivityDisclosurePreferenceContextValue {
  return useContext(ActivityDisclosurePreferenceContext) ?? noopPreference;
}
