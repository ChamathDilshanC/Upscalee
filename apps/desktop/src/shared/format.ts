import type { Dimensions, JobStatus, ModelProfile, OutputFormat, Stage, Warning } from "./api";

export function dims(d: Dimensions | null | undefined): string {
  return d ? `${d.width} × ${d.height}` : "";
}

export function profileLabel(p: ModelProfile): string {
  return p === "photo" ? "Photo" : "Anime";
}

export function formatLabel(f: OutputFormat): string {
  switch (f) {
    case "keep":
      return "Keep";
    case "png":
      return "PNG";
    case "jpg":
      return "JPG";
    case "webp":
      return "WEBP";
  }
}

export function stageLabel(stage: Stage): string {
  switch (stage.kind) {
    case "queued":
      return "Waiting";
    case "preparing":
      return "Preparing";
    case "upscaling":
      return stage.passes > 1 ? `AI upscaling · pass ${stage.pass} of ${stage.passes}` : "AI upscaling";
    case "resizing":
      return "Resizing to exact size";
    case "saving":
      return "Saving";
  }
}

export function duration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)} s`;
  const m = Math.floor(s / 60);
  const rest = Math.round(s % 60);
  return `${m} min ${rest} s`;
}

export function warningText(w: Warning): string {
  switch (w.kind) {
    case "noGpu":
      return "Standard resize — no Vulkan GPU found";
    case "engineMissing":
      return "Standard resize — AI engine not installed";
    case "tinySource":
      return "Very small source — detail is mostly invented";
    case "outputFolderFallback":
      return `Saved to ${w.folder} (folder wasn't writable)`;
  }
}

export function isActive(status: JobStatus): boolean {
  return status.state === "queued" || status.state === "running";
}

export function percent(fraction: number): string {
  return `${Math.round(Math.min(1, Math.max(0, fraction)) * 100)}%`;
}
