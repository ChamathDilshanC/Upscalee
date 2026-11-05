// Typed bridge to the Rust side. Shapes mirror the serde output of
// apps/desktop/src-tauri/src/{radial,jobs,commands}.rs and upscalee-core.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Preset = "2K" | "4K" | "8K" | "12K" | "16K";
export type ModelProfile = "photo" | "anime";
export type OutputFormat = "keep" | "png" | "jpg" | "webp";

export const PRESETS: readonly Preset[] = ["2K", "4K", "8K", "12K", "16K"];
export const PROFILES: readonly ModelProfile[] = ["photo", "anime"];
export const FORMATS: readonly OutputFormat[] = ["keep", "png", "jpg", "webp"];

export interface Dimensions {
  width: number;
  height: number;
}

export type Availability = "ready" | "partial" | "alreadyLarger";

export interface PresetOption {
  preset: Preset;
  longEdge: number;
  availability: Availability;
  eligible: number;
  target: Dimensions | null;
}

export interface FileSummary {
  name: string;
  format: string;
  width: number;
  height: number;
}

export type EngineMode = "ai" | "noGpu" | "missing";

export interface RadialSession {
  id: number;
  count: number;
  summary: string;
  detail: string;
  files: FileSummary[];
  unreadable: string[];
  presets: PresetOption[];
  profile: ModelProfile;
  format: OutputFormat;
  highlighted: Preset | null;
  engine: EngineMode;
}

export type Stage =
  | { kind: "queued" | "preparing" | "resizing" | "saving" }
  | { kind: "upscaling"; pass: number; passes: number };

export type Warning =
  | { kind: "noGpu" | "engineMissing" | "tinySource" }
  | { kind: "outputFolderFallback"; folder: string };

export type JobStatus =
  | { state: "queued" }
  | { state: "running"; stage: Stage; progress: number }
  | {
      state: "done";
      output: string;
      outputName: string;
      dimensions: Dimensions;
      ai: boolean;
      warnings: Warning[];
      elapsedMs: number;
    }
  | { state: "skipped"; reason: string }
  | { state: "failed"; message: string; suggestion: Preset | null }
  | { state: "cancelled" };

export interface JobView {
  id: number;
  fileName: string;
  folder: string;
  preset: Preset | null;
  profile: ModelProfile | null;
  status: JobStatus;
}

export interface Settings {
  outputFolder: string | null;
  outputFormat: OutputFormat;
  defaultProfile: ModelProfile;
  tileSize: number;
  gpu: number | null;
  concurrentJobs: number;
  runAtStartup: boolean;
  skipIfLargeEnough: boolean;
  openFolderWhenDone: boolean;
  lastPreset: Preset | null;
  explorerIntegration: boolean;
}

export type ClassicStatus = "installed" | "outdated" | "missing";
export type ModernStatus =
  | { state: "installed" | "notInstalled" | "packageMissing" | "unsupported" }
  | { state: "error"; message: string };

export interface IntegrationStatus {
  classic: ClassicStatus;
  modern: ModernStatus;
  shellExtension: boolean;
  runAtStartup: boolean;
  windows11: boolean;
  windowsBuild: number;
}

export interface GpuInfo {
  index: number;
  name: string;
}

export interface EngineStatus {
  installed: boolean;
  path: string | null;
  photoModel: boolean;
  animeModel: boolean;
  gpuState: "unknown" | "available" | "missing";
  gpus: GpuInfo[] | null;
}

export interface AppInfo {
  version: string;
  logsDir: string;
  settingsFile: string;
  installDir: string;
  engineVersion: string;
}

export const api = {
  radialCurrent: () => invoke<RadialSession | null>("radial_current"),
  radialReady: (id: number) => invoke<void>("radial_ready", { id }),
  radialSelect: (id: number, preset: Preset, profile: ModelProfile, format: OutputFormat) =>
    invoke<void>("radial_select", { id, preset, profile, format }),
  radialCancel: () => invoke<void>("radial_cancel"),

  hudSetHeight: (height: number) => invoke<void>("hud_set_height", { height }),
  hudHide: () => invoke<void>("hud_hide"),
  jobsSnapshot: () => invoke<JobView[]>("jobs_snapshot"),
  jobCancel: (id: number) => invoke<void>("job_cancel", { id }),
  jobsCancelAll: () => invoke<void>("jobs_cancel_all"),
  jobsClearFinished: () => invoke<void>("jobs_clear_finished"),
  jobOpen: (id: number) => invoke<void>("job_open", { id }),
  jobReveal: (id: number) => invoke<void>("job_reveal", { id }),

  settingsGet: () => invoke<Settings>("settings_get"),
  settingsDefaults: () => invoke<Settings>("settings_defaults"),
  settingsSave: (settings: Settings) => invoke<Settings>("settings_save", { settings }),
  settingsPickFolder: () => invoke<string | null>("settings_pick_folder"),
  integrationStatus: () => invoke<IntegrationStatus>("integration_status"),
  integrationInstall: () => invoke<IntegrationStatus>("integration_install"),
  integrationRemove: () => invoke<IntegrationStatus>("integration_remove"),
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  engineProbe: () => invoke<GpuInfo[]>("engine_probe"),
  appInfo: () => invoke<AppInfo>("app_info"),
  openLogs: () => invoke<void>("open_logs"),
};

export const events = {
  onRadialSession: (cb: (s: RadialSession) => void): Promise<UnlistenFn> =>
    listen<RadialSession>("radial://session", (e) => cb(e.payload)),
  onJobsChanged: (cb: (jobs: JobView[]) => void): Promise<UnlistenFn> =>
    listen<JobView[]>("jobs://changed", (e) => cb(e.payload)),
};

/** Error text from a rejected invoke (Rust commands reject with a string). */
export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Something went wrong";
}
