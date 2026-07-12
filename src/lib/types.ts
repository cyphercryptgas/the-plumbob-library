/**
 * Wire types for the typed command boundary. These mirror the Rust structs'
 * serde output (camelCase) exactly — see core/src/db/*.rs and
 * src-tauri/src/service.rs. If a field changes in Rust, it changes here.
 */

export interface AppInfo {
  name: string;
  tagline: string;
  disclaimer: string;
  version: string;
  dataDir: string;
  dbPath: string;
}

export interface AppSettings {
  modsFolder: string | null;
  dataFolder: string | null;
  backupFolder: string | null;
  quarantineFolder: string | null;
  scanExcluded: string[];
  scriptDepthLimit: number;
  hashOnScan: boolean;
  stopOnError: boolean;
  theme: string;
  reducedMotion: boolean;
  /** Stored only in the local database; used by Patch Center (Phase 3). */
  curseforgeApiKey: string | null;
}

export interface ModsFolderCheck {
  exists: boolean;
  isDirectory: boolean;
  topLevelEntries: number;
  hasResourceCfg: boolean;
  hasSimsFiles: boolean;
}

export interface LibraryCounts {
  totalFiles: number;
  totalBytes: number;
  missing: number;
  zeroByte: number;
  unsupported: number;
  archives: number;
  deepScripts: number;
  packages: number;
  scripts: number;
  quarantined: number;
  disabled: number;
}

export interface FileRow {
  id: number;
  relativePath: string;
  absolutePath: string;
  currentFilename: string;
  fileType: string;
  sizeBytes: number;
  sha256: string | null;
  status: string;
  missing: boolean;
  zeroByte: boolean;
  deepScript: boolean;
  depth: number;
  modifiedAtFs: string | null;
  modId: number | null;
  parseStatus: string | null;
  enabled: boolean;
  category: string | null;
}

export interface DuplicateMemberView {
  fileId: number;
  relativePath: string;
  sizeBytes: number;
  modifiedAtFs: string | null;
  recommended: boolean;
}

export interface DuplicateGroupView {
  id: number;
  sha256: string | null;
  sizeBytes: number | null;
  reclaimableBytes: number;
  recommendedFileId: number | null;
  recommendationReason: string | null;
  members: DuplicateMemberView[];
}

export interface QuarantineView {
  id: number;
  fileId: number | null;
  originalPath: string;
  quarantinePath: string;
  sha256: string | null;
  reason: string;
  quarantinedAt: string;
  restoredAt: string | null;
  status: string;
}

export interface OperationView {
  id: number;
  operationUid: string;
  operationType: string;
  status: string;
  createdAt: string;
  completedAt: string | null;
  summary: string | null;
  backupId: number | null;
}

export interface OperationStepView {
  stepOrder: number;
  action: string;
  sourcePath: string;
  destinationPath: string | null;
  expectedHash: string | null;
  status: string;
  errorMessage: string | null;
}

export interface BackupView {
  id: number;
  createdAt: string;
  reason: string;
  rootPath: string;
  status: string;
  totalFiles: number;
  totalBytes: number;
  operationId: number | null;
}

export interface BackupEntryView {
  sourcePath: string;
  backupPath: string;
  sha256: string;
  sizeBytes: number;
}

export interface ScanProgressEvent {
  phase: "scanning" | "hashing" | "parsing";
  filesSeen: number;
  bytesSeen: number;
  hashed: number;
  toHash: number;
}

export interface ScanOutcome {
  scanId: number;
  newFiles: number;
  changedFiles: number;
  unchangedFiles: number;
  missingFiles: number;
  reappearedFiles: number;
  hashedFiles: number;
  hashErrors: number;
  duplicateGroups: number;
  packagesParsed: number;
  parseErrors: number;
  scanErrors: number;
  cancelled: boolean;
  durationMs: number;
}

export interface QuarantinePreview {
  files: FileRow[];
  totalBytes: number;
  filesWithoutHash: number;
  filesMissingOnDisk: number;
}

export interface FailedStep {
  path: string;
  message: string;
}

export interface QuarantineOutcomeDto {
  operationId: string;
  backupId: number;
  completed: number;
  failed: FailedStep[];
  haltedEarly: boolean;
  reclaimedBytes: number;
}

export type LibraryFilter =
  | "all"
  | "packages"
  | "scripts"
  | "archives"
  | "zero-byte"
  | "deep-scripts"
  | "missing"
  | "quarantined"
  | "disabled"
  | "unreadable"
  | "cat_cas"
  | "cat_buildbuy"
  | "cat_animations"
  | "cat_gameplay"
  | "cat_scripts"
  | "cat_other"
  | "date_7"
  | "date_30"
  | "date_90"
  | "date_old";

export interface ConflictMember {
  fileId: number;
  relativePath: string;
  absolutePath: string;
}

export interface ConflictKey {
  typeId: number;
  tgi: string;
  typeName: string | null;
  presentationOnly: boolean;
}

export interface ConflictGroup {
  /** Ordered by relative path (case-insensitive) — the community-understood
   * load order. The last member is the presumptive winner. */
  members: ConflictMember[];
  sharedKeyCount: number;
  sampleKeys: ConflictKey[];
  severity: "gameplay" | "presentation";
  likelyIntentional: boolean;
}

export interface SuspectedMember {
  fileId: number;
  relativePath: string;
  absolutePath: string;
  sizeBytes: number;
}

export interface SuspectedDuplicateGroup {
  fileName: string;
  members: SuspectedMember[];
}

// ---------------------------------------------------------------------------
// Troubleshooter (the 50/50 assistant)
// ---------------------------------------------------------------------------

export interface TroubleshootCandidate {
  fileId: number;
  relativePath: string;
}

export interface TroubleshootSession {
  id: number;
  status: string;
  phase: string;
  round: number;
  problemNote: string | null;
  outcome: string | null;
  createdAt: string;
  updatedAt: string;
  total: number;
  inCount: number;
  outCount: number;
  poolSize: number;
  candidate: TroubleshootCandidate | null;
}

export interface TroubleshootReconcileReport {
  healed: number;
  conflicts: string[];
  missing: string[];
}

export interface ProfileView {
  id: number;
  name: string;
  createdAt: string;
  updatedAt: string;
  isActive: boolean;
  disabledCount: number;
}

export interface ToggleOutcomeDto {
  completed: number;
  skipped: number;
  failed: { path: string; message: string }[];
}

export interface PlannedToggleDto {
  fileId: number;
  relativePath: string;
  sha256: string | null;
}

export interface SwitchPlanDto {
  toDisable: PlannedToggleDto[];
  toEnable: PlannedToggleDto[];
  unavailable: string[];
}

export interface SwitchOutcomeDto {
  disabledApplied: number;
  enabledApplied: number;
  unavailable: string[];
  failed: { path: string; message: string }[];
  activated: boolean;
}

export interface PatchCheckSummary {
  eligible: number;
  newlyFingerprinted: number;
  rawMatches: number;
  otherGame: number;
  matched: number;
  updates: number;
  unknown: number;
  corpusProbe: boolean | null;
  checkedAt: string;
}

export interface CurseStatusRow {
  fileId: number;
  relativePath: string;
  currentFilename: string;
  enabled: boolean;
  fingerprinted: boolean;
  curseModId: number | null;
  latestFileId: number | null;
  modName: string | null;
  websiteUrl: string | null;
  matchedFileName: string | null;
  matchedFileDate: string | null;
  latestFileName: string | null;
  latestFileDate: string | null;
  updateAvailable: boolean;
  checkedAt: string | null;
}
