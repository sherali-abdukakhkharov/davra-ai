import { useCallback, useEffect, useRef, useState, type DependencyList } from "react";

export type Resource<T> = {
  data: T | undefined;
  error: unknown;
  loading: boolean;
  reload: (opts?: { quiet?: boolean }) => Promise<T | undefined>;
  setData: (fn: (prev: T | undefined) => T | undefined) => void;
};

/** Fetches on mount and when deps change; `reload({quiet})` refreshes without a loading state. */
export function useResource<T>(load: (signal: AbortSignal) => Promise<T>, deps: DependencyList): Resource<T> {
  const [data, setDataState] = useState<T | undefined>(undefined);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const loadRef = useRef(load);
  loadRef.current = load;
  const ctrl = useRef<AbortController | null>(null);

  const reload = useCallback(async (opts?: { quiet?: boolean }) => {
    ctrl.current?.abort();
    const c = new AbortController();
    ctrl.current = c;
    if (!opts?.quiet) setLoading(true);
    try {
      const d = await loadRef.current(c.signal);
      if (c.signal.aborted) return undefined;
      setDataState(d);
      setError(null);
      setLoading(false);
      return d;
    } catch (e) {
      if (c.signal.aborted || (e instanceof DOMException && e.name === "AbortError")) return undefined;
      if (!opts?.quiet) setError(e);
      setLoading(false);
      return undefined;
    }
  }, []);

  useEffect(() => {
    setDataState(undefined);
    void reload();
    return () => ctrl.current?.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  const setData = useCallback((fn: (prev: T | undefined) => T | undefined) => setDataState(fn), []);
  return { data, error, loading, reload, setData };
}
