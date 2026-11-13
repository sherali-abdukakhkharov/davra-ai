import { createContext, useContext } from "react";
import type { Status } from "./bridge";
import type { ReviewRequest } from "./components/ReviewSheet";

export type DesktopState = {
  status: Status;
  setStatus: (s: Status) => void;
};

export const DesktopContext = createContext<DesktopState | null>(null);

export function useDesktop(): DesktopState {
  const ctx = useContext(DesktopContext);
  if (!ctx) throw new Error("useDesktop outside DesktopContext");
  return ctx;
}

/** Needs the connect screen: no hub yet, or the hub didn't confirm who we are. */
export const needsConnect = (s: Status) => !s.hub || !s.me;

/** "https://davra.qitmir.uz/" → "davra.qitmir.uz" */
export function hubHost(hub: string): string {
  try {
    return new URL(hub).host;
  } catch {
    return hub;
  }
}

/** Opens the review sheet (plan → apply) from any panel. */
export const ReviewContext = createContext<((r: ReviewRequest) => void) | null>(null);

export function useReview(): (r: ReviewRequest) => void {
  const open = useContext(ReviewContext);
  if (!open) throw new Error("useReview outside ReviewContext");
  return open;
}
