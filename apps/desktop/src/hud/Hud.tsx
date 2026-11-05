import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorText, events, type JobView } from "../shared/api";
import { dims, duration, isActive, percent, profileLabel, stageLabel, warningText } from "../shared/format";
import { Logo } from "../shared/Logo";

const HIDE_AFTER_MS = 8000;
const HIDE_AFTER_FAILURE_MS = 15000;
const HIDE_WHEN_EMPTY_MS = 3000;

function StatusIcon({ job }: { job: JobView }) {
  switch (job.status.state) {
    case "running":
    case "queued":
      return <span className={`status-icon spinner${job.status.state === "queued" ? " is-queued" : ""}`} aria-hidden />;
    case "done":
      return (
        <svg className="status-icon status-done" viewBox="0 0 16 16" aria-hidden>
          <circle cx="8" cy="8" r="8" />
          <path d="M4.5 8.2 7 10.6l4.6-5" />
        </svg>
      );
    case "failed":
      return (
        <svg className="status-icon status-failed" viewBox="0 0 16 16" aria-hidden>
          <circle cx="8" cy="8" r="8" />
          <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" />
        </svg>
      );
    case "skipped":
    case "cancelled":
      return (
        <svg className="status-icon status-muted" viewBox="0 0 16 16" aria-hidden>
          <circle cx="8" cy="8" r="8" />
          <path d="M5 8h6" />
        </svg>
      );
  }
}

function meta(job: JobView): string {
  const target = [job.preset ? `→ ${job.preset}` : null, job.profile ? profileLabel(job.profile) : null]
    .filter(Boolean)
    .join(" · ");
  const s = job.status;
  switch (s.state) {
    case "queued":
      return `${target} · Waiting`;
    case "running":
      return `${target} · ${stageLabel(s.stage)} · ${percent(s.progress)}`;
    case "done":
      return `${s.outputName} · ${dims(s.dimensions)} · ${duration(s.elapsedMs)}`;
    case "skipped":
      return `Skipped — ${s.reason}`;
    case "failed":
      return s.message;
    case "cancelled":
      return "Cancelled";
  }
}

function title(jobs: JobView[]): string {
  if (jobs.length === 0) return "Upscalee";
  const counted = jobs.filter((j) => j.preset !== null);
  const finished = counted.filter((j) => !isActive(j.status)).length;
  if (jobs.some((j) => isActive(j.status))) {
    return counted.length > 1 ? `Upscaling ${Math.min(finished + 1, counted.length)} of ${counted.length}` : "Upscaling…";
  }
  const done = jobs.filter((j) => j.status.state === "done").length;
  const failed = jobs.filter((j) => j.status.state === "failed").length;
  if (failed > 0) return done > 0 ? `${done} done · ${failed} failed` : failed > 1 ? `${failed} failed` : "Upscale failed";
  if (done > 0) return done > 1 ? `Done — ${done} images saved` : "Done";
  return "Nothing to do";
}

function JobRow({ job, onError }: { job: JobView; onError: (msg: string) => void }) {
  const s = job.status;
  const run = (p: Promise<unknown>) => p.catch((e) => onError(errorText(e)));
  return (
    <li className={`job job-${s.state}`}>
      <StatusIcon job={job} />
      <div className="job-main">
        <div className="job-name" title={`${job.folder}\\${job.fileName}`}>
          {job.fileName}
        </div>
        <div className="job-meta">{meta(job)}</div>
        {s.state === "running" && (
          <div
            className="progress"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(s.progress * 100)}
          >
            <div className="progress-bar" style={{ width: percent(s.progress) }} />
          </div>
        )}
        {s.state === "done" &&
          s.warnings.map((w) => (
            <div className="job-warning" key={w.kind}>
              {warningText(w)}
            </div>
          ))}
        {s.state === "failed" && s.suggestion && <div className="job-warning">Try {s.suggestion} instead.</div>}
      </div>
      <div className="job-actions">
        {isActive(s) && (
          <button className="btn btn-ghost btn-sm" onClick={() => run(api.jobCancel(job.id))}>
            Cancel
          </button>
        )}
        {s.state === "done" && (
          <>
            <button className="btn btn-sm" onClick={() => run(api.jobOpen(job.id))}>
              Open
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => run(api.jobReveal(job.id))}>
              Show in folder
            </button>
          </>
        )}
      </div>
    </li>
  );
}

export function Hud() {
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [hovered, setHovered] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    events.onJobsChanged(setJobs).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    api
      .jobsSnapshot()
      .then((j) => !disposed && setJobs(j))
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // Keep the native window exactly as tall as the content. Measure synchronously
  // after every content change (works while the window is hidden, when
  // ResizeObserver may not fire), plus a ResizeObserver for anything else.
  const lastHeight = useRef(0);
  const reportHeight = () => {
    const el = rootRef.current;
    if (!el) return;
    const height = Math.ceil(el.getBoundingClientRect().height) + 2;
    if (height !== lastHeight.current) {
      lastHeight.current = height;
      api.hudSetHeight(height).catch(() => undefined);
    }
  };
  useLayoutEffect(reportHeight);
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const observer = new ResizeObserver(() => reportHeight());
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const active = jobs.some((j) => isActive(j.status));
  const failed = jobs.some((j) => j.status.state === "failed");

  // Toast-like auto-hide once everything has finished (paused while hovered).
  useEffect(() => {
    if (active || hovered) return;
    const delay = jobs.length === 0 ? HIDE_WHEN_EMPTY_MS : failed ? HIDE_AFTER_FAILURE_MS : HIDE_AFTER_MS;
    const timer = window.setTimeout(() => {
      api.jobsClearFinished().catch(() => undefined);
      api.hudHide().catch(() => undefined);
    }, delay);
    return () => window.clearTimeout(timer);
  }, [active, hovered, failed, jobs.length]);

  useEffect(() => {
    if (!error) return;
    const t = window.setTimeout(() => setError(null), 5000);
    return () => window.clearTimeout(t);
  }, [error]);

  const close = () => {
    if (!active) api.jobsClearFinished().catch(() => undefined);
    api.hudHide().catch(() => undefined);
  };

  return (
    <div
      className="hud"
      ref={rootRef}
      role="region"
      aria-label="Upscalee activity"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <header className="hud-header">
        <Logo size={18} />
        <div className="hud-title" aria-live="polite">
          {title(jobs)}
        </div>
        {active && (
          <button className="btn btn-ghost btn-sm" onClick={() => api.jobsCancelAll().catch(() => undefined)}>
            Cancel all
          </button>
        )}
        <button className="btn btn-ghost btn-icon btn-sm" aria-label="Close" onClick={close}>
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden>
            <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
      </header>
      {jobs.length === 0 ? (
        <div className="hud-empty">No recent activity</div>
      ) : (
        <ul className="hud-list">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} onError={setError} />
          ))}
        </ul>
      )}
      {error && (
        <div className="hud-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
