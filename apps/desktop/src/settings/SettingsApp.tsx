import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  errorText,
  type AppInfo,
  type EngineStatus,
  type IntegrationStatus,
  type ModelProfile,
  type OutputFormat,
  type Settings,
} from "../shared/api";
import { Logo } from "../shared/Logo";
import { Row, Section, Segmented, Select, Toggle } from "./controls";

type SaveState = { kind: "idle" } | { kind: "saving" } | { kind: "saved" } | { kind: "error"; message: string };

const FORMAT_OPTIONS: { value: OutputFormat; label: string }[] = [
  { value: "keep", label: "Keep original" },
  { value: "png", label: "PNG (lossless)" },
  { value: "jpg", label: "JPG (quality 95)" },
  { value: "webp", label: "WEBP (quality 95)" },
];

const PROFILE_OPTIONS: { value: ModelProfile; label: string }[] = [
  { value: "photo", label: "Photo" },
  { value: "anime", label: "Anime / art" },
];

const TILE_OPTIONS = [
  { value: 0, label: "Automatic" },
  { value: 512, label: "512 px" },
  { value: 400, label: "400 px" },
  { value: 256, label: "256 px" },
  { value: 200, label: "200 px" },
  { value: 128, label: "128 px" },
  { value: 64, label: "64 px (low VRAM)" },
];

const CONCURRENCY_OPTIONS = [1, 2, 3, 4].map((n) => ({ value: n, label: n === 1 ? "1 (recommended)" : String(n) }));

function ClassicPill({ status }: { status: IntegrationStatus }) {
  switch (status.classic) {
    case "installed":
      return <span className="pill pill-success">Installed</span>;
    case "outdated":
      return <span className="pill pill-warning">Needs repair</span>;
    case "missing":
      return <span className="pill pill-danger">Not installed</span>;
  }
}

function ModernPill({ status }: { status: IntegrationStatus }) {
  switch (status.modern.state) {
    case "installed":
      return <span className="pill pill-success">Installed</span>;
    case "notInstalled":
      return <span className="pill pill-warning">Not installed</span>;
    case "packageMissing":
      return <span className="pill">Needs signed package</span>;
    case "unsupported":
      return <span className="pill">Windows 11 only</span>;
    case "error":
      return (
        <span className="pill pill-danger" title={status.modern.message}>
          Error
        </span>
      );
  }
}

function gpuLabel(engine: EngineStatus | null): string {
  if (!engine) return "Checking…";
  if (!engine.installed) return "AI engine not installed — images use a standard Lanczos resize.";
  switch (engine.gpuState) {
    case "available":
      return "Vulkan GPU ready.";
    case "missing":
      return "No Vulkan GPU found — standard resize is used instead of AI.";
    case "unknown":
      return "GPU is checked on the first upscale (or press Detect).";
  }
}

export function SettingsApp() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saveState, setSaveState] = useState<SaveState>({ kind: "idle" });
  const [integration, setIntegration] = useState<IntegrationStatus | null>(null);
  const [integrationBusy, setIntegrationBusy] = useState(false);
  const [integrationError, setIntegrationError] = useState<string | null>(null);
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [probing, setProbing] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api.settingsGet().then(setSettings).catch((e) => setSaveState({ kind: "error", message: errorText(e) }));
    api.integrationStatus().then(setIntegration).catch((e) => setIntegrationError(errorText(e)));
    api.engineStatus().then(setEngine).catch(() => undefined);
    api.appInfo().then(setInfo).catch(() => undefined);
  }, []);

  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((current) => {
      if (!current) return current;
      const next = { ...current, ...patch };
      window.clearTimeout(saveTimer.current);
      setSaveState({ kind: "saving" });
      saveTimer.current = window.setTimeout(() => {
        api
          .settingsSave(next)
          .then((saved) => {
            setSettings(saved);
            setSaveState({ kind: "saved" });
          })
          .catch((e) => setSaveState({ kind: "error", message: errorText(e) }));
      }, 250);
      return next;
    });
  }, []);

  const runIntegration = (action: () => Promise<IntegrationStatus>) => {
    setIntegrationBusy(true);
    setIntegrationError(null);
    action()
      .then((status) => {
        setIntegration(status);
        return api.settingsGet().then(setSettings);
      })
      .catch((e) => setIntegrationError(errorText(e)))
      .finally(() => setIntegrationBusy(false));
  };

  const probe = () => {
    setProbing(true);
    setProbeError(null);
    api
      .engineProbe()
      .then(() => api.engineStatus().then(setEngine))
      .catch((e) => setProbeError(errorText(e)))
      .finally(() => setProbing(false));
  };

  const browse = () => {
    api
      .settingsPickFolder()
      .then((folder) => folder && update({ outputFolder: folder }))
      .catch(() => undefined);
  };

  if (!settings) {
    return (
      <main className="settings">
        <p className="loading">{saveState.kind === "error" ? saveState.message : "Loading…"}</p>
      </main>
    );
  }

  const gpuOptions = [
    { value: -1, label: "Automatic" },
    ...(engine?.gpus ?? []).map((g) => ({ value: g.index, label: `${g.index}: ${g.name}` })),
  ];
  if (settings.gpu !== null && !gpuOptions.some((o) => o.value === settings.gpu)) {
    gpuOptions.push({ value: settings.gpu, label: `GPU ${settings.gpu}` });
  }

  return (
    <main className="settings">
      <header className="hero">
        <Logo size={44} title="Upscalee" />
        <div>
          <h1 className="hero-title">Upscalee</h1>
          <p className="hero-subtitle">
            Right-click any image in File Explorer → <strong>Upscale with Upscalee</strong> → pick 2K, 4K, 8K, 12K or 16K.
          </p>
        </div>
        <div className={`save-state save-${saveState.kind}`} role="status" aria-live="polite">
          {saveState.kind === "saving" && "Saving…"}
          {saveState.kind === "saved" && "All changes saved"}
          {saveState.kind === "error" && saveState.message}
        </div>
      </header>

      <Section title="Output">
        <Row label="Save upscaled images" hint={settings.outputFolder ?? "Next to the original image"}>
          <div className="inline">
            <Segmented
              label="Output location"
              value={settings.outputFolder ? "custom" : "source"}
              options={[
                { value: "source", label: "Same folder" },
                { value: "custom", label: "Custom folder" },
              ]}
              onChange={(v) => (v === "source" ? update({ outputFolder: null }) : browse())}
            />
            {settings.outputFolder && (
              <button className="btn" onClick={browse}>
                Change…
              </button>
            )}
          </div>
        </Row>
        <Row label="Output format" hint="16K PNGs can be 100–300 MB; JPG or WEBP are much smaller." htmlFor="format">
          <Select id="format" value={settings.outputFormat} options={FORMAT_OPTIONS} onChange={(v) => update({ outputFormat: v })} />
        </Row>
      </Section>

      <Section title="Upscaling">
        <Row label="Default model" hint="Press Tab on the radial menu to switch per image.">
          <Segmented
            label="Default model"
            value={settings.defaultProfile}
            options={PROFILE_OPTIONS}
            onChange={(v) => update({ defaultProfile: v })}
          />
        </Row>
        <Row label="Skip images that are already large enough" hint='Shows "Already 4K or larger" instead of re-rendering.'>
          <Toggle
            label="Skip images that are already large enough"
            checked={settings.skipIfLargeEnough}
            onChange={(v) => update({ skipIfLargeEnough: v })}
          />
        </Row>
        <Row label="Tile size" hint="Smaller tiles use less video memory; Automatic retries smaller tiles if needed." htmlFor="tile">
          <Select id="tile" value={settings.tileSize} options={TILE_OPTIONS} onChange={(v) => update({ tileSize: v })} />
        </Row>
        <Row label="GPU" hint={gpuLabel(engine)} htmlFor="gpu">
          <div className="inline">
            <Select
              id="gpu"
              value={settings.gpu ?? -1}
              options={gpuOptions}
              onChange={(v) => update({ gpu: v < 0 ? null : v })}
            />
            <button className="btn" onClick={probe} disabled={probing || !engine?.installed}>
              {probing ? "Detecting…" : "Detect"}
            </button>
          </div>
        </Row>
        {probeError && <p className="inline-error">{probeError}</p>}
        <Row label="Concurrent jobs" hint="Each 16K job can use 1–2 GB of RAM." htmlFor="concurrency">
          <Select
            id="concurrency"
            value={settings.concurrentJobs}
            options={CONCURRENCY_OPTIONS}
            onChange={(v) => update({ concurrentJobs: v })}
          />
        </Row>
      </Section>

      <Section title="System">
        <Row label="Run at startup" hint="Keeps Upscalee in the tray so the menu opens instantly.">
          <Toggle label="Run at startup" checked={settings.runAtStartup} onChange={(v) => update({ runAtStartup: v })} />
        </Row>
        <Row label="Open folder when done" hint="Selects the last saved image in File Explorer.">
          <Toggle
            label="Open folder when done"
            checked={settings.openFolderWhenDone}
            onChange={(v) => update({ openFolderWhenDone: v })}
          />
        </Row>
      </Section>

      <Section
        title="File Explorer integration"
        description='Adds "Upscale with Upscalee" to the right-click menu for PNG, JPG, WEBP, BMP and TIFF images.'
      >
        {integration ? (
          <>
            <Row
              label="Context menu"
              hint={
                integration.windows11
                  ? 'Shown under "Show more options" (Shift + F10).'
                  : "Shown in the right-click menu."
              }
            >
              <ClassicPill status={integration} />
            </Row>
            <Row
              label="Windows 11 modern menu"
              hint={
                integration.modern.state === "packageMissing"
                  ? "Requires the signed context-menu package (see docs/DEPLOYMENT.md)."
                  : integration.modern.state === "error"
                    ? integration.modern.message
                    : "Top-level entry in the Windows 11 menu."
              }
            >
              <ModernPill status={integration} />
            </Row>
            <div className="row row-actions">
              {integrationError && <p className="inline-error">{integrationError}</p>}
              <button
                className="btn btn-primary"
                disabled={integrationBusy}
                onClick={() => runIntegration(api.integrationInstall)}
              >
                {integration.classic === "installed" ? "Repair" : "Install"}
              </button>
              <button
                className="btn btn-danger"
                disabled={integrationBusy || integration.classic === "missing"}
                onClick={() => runIntegration(api.integrationRemove)}
              >
                Remove
              </button>
            </div>
          </>
        ) : (
          <p className="loading">{integrationError ?? "Checking…"}</p>
        )}
      </Section>

      <Section title="About">
        <Row label="Version" hint={info?.engineVersion}>
          <span className="mono">{info?.version ?? "…"}</span>
        </Row>
        <Row label="Logs" hint={info?.logsDir}>
          <button className="btn" onClick={() => api.openLogs().catch(() => undefined)}>
            Open logs folder
          </button>
        </Row>
        <p className="legal">
          Fully offline — no network access, no telemetry. AI upscaling invents plausible detail; it cannot recover
          detail that was never captured. Uses Real-ESRGAN (BSD-3-Clause) and ncnn (BSD-3-Clause); see
          THIRD_PARTY_NOTICES.md in the install folder.
        </p>
      </Section>
    </main>
  );
}
