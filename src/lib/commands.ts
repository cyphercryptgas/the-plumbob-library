/**
 * Typed wrappers for every shell command — the only place command names and
 * argument shapes appear. Tauri maps these camelCase argument keys onto the
 * Rust snake_case parameters.
 */
import { call } from "./tauri";
import type {
  AppInfo,
  AppSettings,
  BackupEntryView,
  BackupView,
  ConflictGroup,
  DuplicateGroupView,
  FileRow,
  LibraryFilter,
  LibraryCounts,
  ModsFolderCheck,
  OperationStepView,
  OperationView,
  QuarantineOutcomeDto,
  QuarantinePreview,
  QuarantineView,
  ScanOutcome,
  SuspectedDuplicateGroup,
  TroubleshootReconcileReport,
  TroubleshootSession,
  ProfileView,
  ToggleOutcomeDto,
  SwitchPlanDto,
  SwitchOutcomeDto,
  PatchCheckSummary,
  CurseStatusRow,
} from "./types";

// App identity & settings -----------------------------------------------

export const appInfo = () => call<AppInfo>("app_info");
export const getSettings = () => call<AppSettings>("get_settings");
export const saveSettings = (settings: AppSettings) =>
  call<void>("save_settings", { settings });

// Onboarding helpers -----------------------------------------------------

export const detectModsFolder = () =>
  call<string | null>("detect_mods_folder");
export const validateModsFolder = (path: string) =>
  call<ModsFolderCheck>("validate_mods_folder", { path });
export const gameRunning = () => call<boolean>("game_running");

// Scan lifecycle ---------------------------------------------------------

export const startScan = (scanType?: string) =>
  call<ScanOutcome>("start_scan", { scanType: scanType ?? null });
export const cancelScan = () => call<void>("cancel_scan");

// Library queries --------------------------------------------------------

export const getLibraryCounts = () =>
  call<LibraryCounts>("get_library_counts");
export const listFiles = (options?: {
  search?: string;
  filter?: LibraryFilter;
  creator?: string;
  sort?: string;
  limit?: number;
  offset?: number;
}) =>
  call<FileRow[]>("list_files", {
    search: options?.search ?? null,
    filter: options?.filter ?? null,
    creator: options?.creator ?? null,
    sort: options?.sort ?? null,
    limit: options?.limit ?? null,
    offset: options?.offset ?? null,
  });
export const countFiles = (options?: {
  search?: string;
  filter?: LibraryFilter;
  creator?: string;
}) =>
  call<number>("count_files", {
    search: options?.search ?? null,
    filter: options?.filter ?? null,
    creator: options?.creator ?? null,
  });
export const listDuplicateGroups = () =>
  call<DuplicateGroupView[]>("list_duplicate_groups");
export const listConflicts = () => call<ConflictGroup[]>("list_conflicts");
export const listSuspectedDuplicates = () =>
  call<SuspectedDuplicateGroup[]>("list_suspected_duplicates");
export const setDuplicateGroupStatus = (
  groupId: number,
  status: "open" | "resolved" | "dismissed"
) => call<void>("set_duplicate_group_status", { groupId, status });

// Quarantine & restore ----------------------------------------------------

export const previewQuarantine = (fileIds: number[]) =>
  call<QuarantinePreview>("preview_quarantine", { fileIds });
export const executeQuarantine = (
  fileIds: number[],
  reason: string,
  resolveGroupId?: number
) =>
  call<QuarantineOutcomeDto>("execute_quarantine", {
    fileIds,
    reason,
    resolveGroupId: resolveGroupId ?? null,
  });
export const restoreQuarantinedFile = (entryId: number) =>
  call<string>("restore_quarantined_file", { entryId });
export const listQuarantine = (includeRestored = false) =>
  call<QuarantineView[]>("list_quarantine", { includeRestored });

// Backups & activity -------------------------------------------------------

export const listBackups = () => call<BackupView[]>("list_backups");
export const listBackupEntries = (backupId: number) =>
  call<BackupEntryView[]>("list_backup_entries", { backupId });
export const restoreBackupEntry = (
  backupId: number,
  sourcePath: string,
  overwrite = false
) =>
  call<string>("restore_backup_entry", { backupId, sourcePath, overwrite });
export const listOperations = (limit?: number) =>
  call<OperationView[]>("list_operations", { limit: limit ?? null });
export const listOperationSteps = (operationRowId: number) =>
  call<OperationStepView[]>("list_operation_steps", { operationRowId });

// Reveal -------------------------------------------------------------------

export const revealInExplorer = (path: string) =>
  call<void>("reveal_in_explorer", { path });

// --- Troubleshooter (the 50/50 assistant) ----------------------------------

export const troubleshootActive = () =>
  call<TroubleshootSession | null>("troubleshoot_active");

export const troubleshootStart = (note?: string) =>
  call<TroubleshootSession>("troubleshoot_start", { note: note ?? null });

export const troubleshootVerdict = (sessionId: number, problemPresent: boolean) =>
  call<TroubleshootSession>("troubleshoot_verdict", { sessionId, problemPresent });

export const troubleshootAbort = (sessionId: number) =>
  call<TroubleshootSession>("troubleshoot_abort", { sessionId });

export const troubleshootReconcile = (sessionId: number) =>
  call<TroubleshootReconcileReport>("troubleshoot_reconcile", { sessionId });

// --- Profiles ---------------------------------------------------------------

export const listProfiles = () => call<ProfileView[]>("list_profiles");

export const activeProfile = () =>
  call<ProfileView | null>("active_profile");

export const createProfile = (name: string) =>
  call<ProfileView>("create_profile", { name });

export const renameProfile = (profileId: number, name: string) =>
  call<void>("rename_profile", { profileId, name });

export const setActiveProfile = (profileId: number) =>
  call<void>("set_active_profile", { profileId });

export const deleteProfile = (profileId: number) =>
  call<void>("delete_profile", { profileId });

export const setFilesEnabled = (fileIds: number[], enabled: boolean) =>
  call<ToggleOutcomeDto>("set_files_enabled", { fileIds, enabled });

export const previewSwitchProfile = (profileId: number) =>
  call<SwitchPlanDto>("preview_switch_profile", { profileId });

export const switchProfile = (profileId: number) =>
  call<SwitchOutcomeDto>("switch_profile", { profileId });

// --- Patch Center ------------------------------------------------------------

export const checkCurseUpdates = () =>
  call<PatchCheckSummary>("check_curse_updates");

export const curseStatus = () => call<CurseStatusRow[]>("curse_status");

export const openExternal = (url: string) =>
  call<void>("open_external", { url });

export interface ThumbDto {
  fileId: number;
  path: string | null;
}

export const getThumbnails = (fileIds: number[]) =>
  call<ThumbDto[]>("get_thumbnails", { fileIds });

export interface PrepareOutcome {
  generated: number;
  cached: number;
  noImage: number;
}

export const prepareThumbnails = () =>
  call<PrepareOutcome>("prepare_thumbnails");

export interface CensusRow {
  typeHex: string;
  name: string;
  files: number;
}

export interface CensusReport {
  rows: CensusRow[];
  casProbe: { versions: string[]; verdict: string };
}

export const thumbnailCensus = () =>
  call<CensusReport>("thumbnail_census");

export interface CreatorRow {
  key: string;
  display: string;
  files: number;
  onCurse: number;
}

export const creatorsOverview = () =>
  call<CreatorRow[]>("creators_overview");

export interface ReverifyOutcome {
  examined: number;
  kept: number;
  boosted: number;
  dropped: number;
}

export const reverifyMatches = () =>
  call<ReverifyOutcome>("reverify_matches");

export interface UpdateOutcome {
  fileId: number;
  bytes: number;
  fileName: string;
  overlaps: string[];
}

export const applyUpdate = (fileId: number) =>
  call<UpdateOutcome>("apply_update", { fileId });

export interface MergeStats {
  sources: number;
  resourcesIn: number;
  resourcesOut: number;
  collisions: number;
}

export interface MergeOutcome {
  mergedName: string;
  stats: MergeStats;
  skipped: string[];
}

export const mergeFiles = (fileIds: number[], label?: string) =>
  call<MergeOutcome>("merge_files", { fileIds, label: label ?? null });

export interface MergeGroup {
  label: string;
  fileIds: number[];
  files: number;
  bytes: number;
}

export interface AutoMergePlan {
  groups: MergeGroup[];
  totalFiles: number;
  skippedMatched: number;
  skippedDisabled: number;
  skippedUnreadable: number;
  unreadableNames: string[];
  skippedNonCas: number;
}

export const planAutoMerge = () => call<AutoMergePlan>("plan_auto_merge");

export interface TitleItem {
  fileId: number;
  from: string;
  to: string;
}

export interface TitlePlan {
  items: TitleItem[];
  skipped: [string, string][];
}

export interface TitleOutcome {
  renamed: number;
  skipped: [string, string][];
  examples: TitleItem[];
}

export const titlePlan = (fileIds: number[] | null, today: boolean) =>
  call<TitlePlan>("title_plan", { fileIds, today });

export const titleApply = (fileIds: number[] | null, today: boolean) =>
  call<TitleOutcome>("title_apply", { fileIds, today });

export interface MergeModeStatus {
  active: boolean;
  files: number;
  groups: number;
  legacy: boolean;
}

export interface MergeModeOutcome {
  groupsDone: number;
  filesMerged: number;
  resources: number;
  failures: string[];
}

export interface UnMergeOutcome {
  restored: number;
  skipped: number;
  outputsRemoved: number;
}

export const mergeModeStatus = () => call<MergeModeStatus>("merge_mode_status");
export const autoMergeRun = () => call<MergeModeOutcome>("auto_merge_run");
export const unMergeRun = () => call<UnMergeOutcome>("un_merge_run");
