/**
 * Environment detection and the safe invoke wrapper. Outside the desktop
 * shell (e.g. a plain browser preview of the Vite build) commands cannot
 * run — the app shows an honest notice instead of pretending.
 */
import { invoke } from "@tauri-apps/api/core";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const OUTSIDE_SHELL_MESSAGE =
  "This preview is running in a browser, not the desktop app, so library data and file operations aren't available here.";

export async function call<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauri()) {
    throw new Error(OUTSIDE_SHELL_MESSAGE);
  }
  return invoke<T>(command, args);
}

/** Command errors arrive as plain strings from Rust; normalize anything. */
export function asMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something unexpected went wrong.";
}
