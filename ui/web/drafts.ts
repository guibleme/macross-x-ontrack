import {
  createContext,
  useContext,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
export type DraftRecord = { value: unknown; dirty: boolean };
export const DraftContext = createContext<Map<string, DraftRecord> | null>(
  null,
);
export function useDraftValue<T>(
  key: string,
  initial: T,
  isDirty: (value: T) => boolean,
): [T, Dispatch<SetStateAction<T>>] {
  const registry = useContext(DraftContext);
  const [value, setValue] = useState<T>(() =>
    registry?.has(key) ? (registry.get(key)!.value as T) : initial,
  );
  const update: Dispatch<SetStateAction<T>> = (next) =>
    setValue((previous) => {
      const resolved =
        typeof next === "function" ? (next as (value: T) => T)(previous) : next;
      registry?.set(key, { value: resolved, dirty: isDirty(resolved) });
      return resolved;
    });
  return [value, update];
}
export const hasText = (value: string) => Boolean(value.trim());
export const neverDirty = () => false;
