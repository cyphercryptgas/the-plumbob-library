/**
 * Typed event subscriptions for the shell's live channels.
 * Each helper returns an unsubscribe function.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "./tauri";
import type { ScanOutcome, ScanProgressEvent } from "./types";

type Unsub = () => void;

function subscribe<T>(event: string, handler: (payload: T) => void): Unsub {
  if (!isTauri()) return () => {};
  let unlisten: UnlistenFn | null = null;
  let disposed = false;
  listen<T>(event, (e) => handler(e.payload)).then((fn) => {
    if (disposed) fn();
    else unlisten = fn;
  });
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

export function onScanProgress(handler: (p: ScanProgressEvent) => void): Unsub {
  return subscribe("scan://progress", handler);
}

export function onScanCompleted(handler: (o: ScanOutcome) => void): Unsub {
  return subscribe("scan://completed", handler);
}

export function onLibraryChanged(handler: (kind: string) => void): Unsub {
  return subscribe("library://changed", handler);
}

export interface TroubleshootProgress {
  done: number;
  total: number;
}

export function onTroubleshootProgress(
  handler: (p: TroubleshootProgress) => void
): Unsub {
  return subscribe("troubleshoot://progress", handler);
}
