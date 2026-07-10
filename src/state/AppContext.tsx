/**
 * App-wide state: settings, library counts, duplicate summary, game status,
 * and the scan lifecycle (wired to the shell's live events). One provider,
 * no external state library.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import * as api from "../lib/commands";
import { onLibraryChanged, onScanCompleted, onScanProgress } from "../lib/events";
import { asMessage } from "../lib/tauri";
import type {
  AppInfo,
  AppSettings,
  LibraryCounts,
  ScanOutcome,
  ScanProgressEvent,
} from "../lib/types";

export interface ScanState {
  running: boolean;
  progress: ScanProgressEvent | null;
  lastOutcome: ScanOutcome | null;
}

interface DuplicateSummary {
  openGroups: number;
  reclaimableBytes: number;
}

interface AppContextValue {
  loading: boolean;
  /** Bumps whenever library data changes (scans, quarantines, restores) —
   * list screens reload on it. */
  libraryVersion: number;
  info: AppInfo | null;
  settings: AppSettings | null;
  counts: LibraryCounts | null;
  duplicates: DuplicateSummary;
  isGameRunning: boolean;
  scan: ScanState;
  error: string | null;
  clearError: () => void;
  reportError: (e: unknown) => void;
  refreshAll: () => Promise<void>;
  refreshCounts: () => Promise<void>;
  saveSettings: (next: AppSettings) => Promise<boolean>;
  startScan: (scanType?: string) => Promise<void>;
  cancelScan: () => Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside <AppProvider>");
  return ctx;
}

export function AppProvider(props: { children: ReactNode }) {
  const [loading, setLoading] = useState(true);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [counts, setCounts] = useState<LibraryCounts | null>(null);
  const [duplicates, setDuplicates] = useState<DuplicateSummary>({
    openGroups: 0,
    reclaimableBytes: 0,
  });
  const [isGameRunning, setGameRunning] = useState(false);
  const [scan, setScan] = useState<ScanState>({
    running: false,
    progress: null,
    lastOutcome: null,
  });
  const [error, setError] = useState<string | null>(null);
  const [libraryVersion, setLibraryVersion] = useState(0);
  const scanRunning = useRef(false);

  const reportError = useCallback((e: unknown) => setError(asMessage(e)), []);
  const clearError = useCallback(() => setError(null), []);

  const refreshCounts = useCallback(async () => {
    try {
      const [nextCounts, groups] = await Promise.all([
        api.getLibraryCounts(),
        api.listDuplicateGroups(),
      ]);
      setCounts(nextCounts);
      setDuplicates({
        openGroups: groups.length,
        reclaimableBytes: groups.reduce((sum, g) => sum + g.reclaimableBytes, 0),
      });
      setLibraryVersion((v) => v + 1);
    } catch (e) {
      reportError(e);
    }
  }, [reportError]);

  const refreshGame = useCallback(async () => {
    try {
      setGameRunning(await api.gameRunning());
    } catch {
      // Non-fatal; the guard still runs server-side on every mutation.
    }
  }, []);

  const refreshAll = useCallback(async () => {
    try {
      const [nextInfo, nextSettings] = await Promise.all([
        api.appInfo(),
        api.getSettings(),
      ]);
      setInfo(nextInfo);
      setSettings(nextSettings);
      if (nextSettings.modsFolder) {
        await refreshCounts();
      }
      await refreshGame();
    } catch (e) {
      reportError(e);
    } finally {
      setLoading(false);
    }
  }, [refreshCounts, refreshGame, reportError]);

  const saveSettings = useCallback(
    async (next: AppSettings): Promise<boolean> => {
      try {
        await api.saveSettings(next);
        setSettings(next);
        return true;
      } catch (e) {
        reportError(e);
        return false;
      }
    },
    [reportError]
  );

  const startScan = useCallback(
    async (scanType?: string) => {
      if (scanRunning.current) return;
      scanRunning.current = true;
      setScan((s) => ({ ...s, running: true, progress: null }));
      try {
        const outcome = await api.startScan(scanType);
        setScan({ running: false, progress: null, lastOutcome: outcome });
        await refreshCounts();
      } catch (e) {
        setScan((s) => ({ ...s, running: false, progress: null }));
        reportError(e);
      } finally {
        scanRunning.current = false;
      }
    },
    [refreshCounts, reportError]
  );

  const cancelScan = useCallback(async () => {
    try {
      await api.cancelScan();
    } catch (e) {
      reportError(e);
    }
  }, [reportError]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  // Live channels from the shell.
  useEffect(() => {
    const offProgress = onScanProgress((p) =>
      setScan((s) => ({ ...s, running: true, progress: p }))
    );
    const offCompleted = onScanCompleted((o) =>
      setScan({ running: false, progress: null, lastOutcome: o })
    );
    const offChanged = onLibraryChanged(() => {
      void refreshCounts();
    });
    return () => {
      offProgress();
      offCompleted();
      offChanged();
    };
  }, [refreshCounts]);

  // Light game-status polling; the authoritative guard is in Rust.
  useEffect(() => {
    const id = window.setInterval(() => void refreshGame(), 20000);
    const onFocus = () => void refreshGame();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("focus", onFocus);
    };
  }, [refreshGame]);

  const value = useMemo<AppContextValue>(
    () => ({
      loading,
      libraryVersion,
      info,
      settings,
      counts,
      duplicates,
      isGameRunning,
      scan,
      error,
      clearError,
      reportError,
      refreshAll,
      refreshCounts,
      saveSettings,
      startScan,
      cancelScan,
    }),
    [
      loading,
      libraryVersion,
      info,
      settings,
      counts,
      duplicates,
      isGameRunning,
      scan,
      error,
      clearError,
      reportError,
      refreshAll,
      refreshCounts,
      saveSettings,
      startScan,
      cancelScan,
    ]
  );

  return <AppContext.Provider value={value}>{props.children}</AppContext.Provider>;
}
