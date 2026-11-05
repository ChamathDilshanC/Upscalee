import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  FORMATS,
  PRESETS,
  PROFILES,
  api,
  errorText,
  events,
  type ModelProfile,
  type OutputFormat,
  type PresetOption,
  type RadialSession,
} from "../shared/api";
import { dims, formatLabel, profileLabel } from "../shared/format";
import {
  CENTER,
  FORMAT_ARCS,
  OUTER_ARCS,
  PROFILE_ARCS,
  RADII,
  arcMid,
  polar,
  ringPath,
  sectorPath,
} from "./geometry";
import { cycle, keyToAction, stepEnabled } from "./keyboard";

const OUTER_LABEL_R = (RADII.outerFrom + RADII.outerTo) / 2;
const INNER_LABEL_R = (RADII.innerFrom + RADII.innerTo) / 2;

function presetSubLabel(option: PresetOption, count: number): string {
  switch (option.availability) {
    case "alreadyLarger":
      return "Already larger";
    case "partial":
      return `${option.eligible} of ${count}`;
    case "ready":
      return option.target ? `${option.target.width}×${option.target.height}` : `${option.longEdge} px`;
  }
}

function engineNotice(session: RadialSession): string | null {
  switch (session.engine) {
    case "ai":
      return null;
    case "noGpu":
      return "No GPU · standard resize";
    case "missing":
      return "AI engine missing";
  }
}

export function RadialMenu() {
  const [session, setSession] = useState<RadialSession | null>(null);
  const [profile, setProfile] = useState<ModelProfile>("photo");
  const [format, setFormat] = useState<OutputFormat>("keep");
  const [highlight, setHighlight] = useState(-1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sessionRef = useRef<RadialSession | null>(null);

  const apply = useCallback((next: RadialSession) => {
    sessionRef.current = next;
    setSession(next);
    setProfile(next.profile);
    setFormat(next.format);
    setHighlight(next.highlighted ? PRESETS.indexOf(next.highlighted) : -1);
    setBusy(false);
    setError(null);
  }, []);

  // Tell Rust to show the window once the new session is committed to the DOM.
  // (Not requestAnimationFrame: the window is hidden at this point and hidden
  // pages may not get animation frames, which would delay the menu.)
  const sessionId = session?.id;
  useEffect(() => {
    if (sessionId !== undefined) api.radialReady(sessionId).catch(() => undefined);
  }, [sessionId]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    events.onRadialSession(apply).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    api
      .radialCurrent()
      .then((s) => {
        if (s && !disposed && sessionRef.current?.id !== s.id) apply(s);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [apply]);

  const enabled = session ? session.presets.map((p) => p.availability !== "alreadyLarger") : [];

  const select = useCallback(
    (index: number) => {
      const current = sessionRef.current;
      if (!current || busy || index < 0) return;
      if (current.presets[index]?.availability === "alreadyLarger") return;
      setBusy(true);
      api.radialSelect(current.id, PRESETS[index], profile, format).catch((err) => {
        setError(errorText(err));
        setBusy(false);
      });
    },
    [busy, profile, format],
  );

  const cancel = useCallback(() => {
    api.radialCancel().catch(() => undefined);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const action = keyToAction(e.key, e.shiftKey);
      if (!action || !sessionRef.current) return;
      e.preventDefault();
      switch (action.type) {
        case "select":
          select(PRESETS.indexOf(action.preset));
          break;
        case "move":
          setHighlight((h) => stepEnabled(h, action.delta, enabled));
          break;
        case "confirm":
          select(highlight);
          break;
        case "cancel":
          cancel();
          break;
        case "cycleProfile":
          setProfile((p) => cycle(PROFILES, p, action.delta));
          break;
        case "cycleFormat":
          setFormat((f) => cycle(FORMATS, f, action.delta));
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [select, cancel, highlight, enabled]);

  const onBackdrop = (e: ReactPointerEvent) => {
    if ((e.target as Element).closest("[data-hit]")) return;
    cancel();
  };

  if (!session) {
    return <div className="radial" />;
  }

  const hovered = highlight >= 0 ? session.presets[highlight] : null;
  const notice = engineNotice(session);
  const centerDetail = hovered?.target
    ? `→ ${dims(hovered.target)}`
    : hovered
      ? `→ ${hovered.longEdge} px long edge`
      : session.detail;

  return (
    <div className="radial" onPointerDown={onBackdrop}>
      <svg
        key={session.id}
        className={`radial-svg${busy ? " is-busy" : ""}`}
        viewBox="0 0 420 420"
        role="menu"
        aria-label={`Upscale ${session.summary}`}
      >
        <circle data-hit className="disc" cx={CENTER} cy={CENTER} r={RADII.disc} />
        <path className="inner-band" d={ringPath(RADII.innerFrom - 2, RADII.innerTo + 2)} />

        {PRESETS.map((preset, i) => {
          const option = session.presets[i];
          const arc = OUTER_ARCS[i];
          const label = polar(OUTER_LABEL_R, arcMid(arc));
          const disabled = option.availability === "alreadyLarger";
          const classes = [
            "wedge",
            "wedge-quality",
            i === highlight ? "is-highlighted" : "",
            disabled ? "is-disabled" : "",
            option.availability === "partial" ? "is-partial" : "",
          ].join(" ");
          return (
            <g
              key={preset}
              data-hit
              className={classes}
              role="menuitem"
              aria-disabled={disabled}
              aria-label={`${preset}${disabled ? ", already larger" : ""}`}
              onPointerEnter={() => !disabled && setHighlight(i)}
              onClick={() => select(i)}
            >
              <path className="wedge-shape" d={sectorPath(RADII.outerFrom, RADII.outerTo, arc, 4)} />
              <text className="wedge-label" x={label.x} y={label.y - 7}>
                {preset}
              </text>
              <text className="wedge-sub" x={label.x} y={label.y + 13}>
                {presetSubLabel(option, session.count)}
              </text>
            </g>
          );
        })}

        {PROFILES.map((p, i) => {
          const arc = PROFILE_ARCS[i];
          const label = polar(INNER_LABEL_R, arcMid(arc));
          const active = p === profile;
          return (
            <g
              key={p}
              data-hit
              className={`wedge wedge-option${active ? " is-active" : ""}`}
              role="menuitemradio"
              aria-checked={active}
              aria-label={`Model: ${profileLabel(p)}`}
              onClick={() => setProfile(p)}
            >
              <path className="wedge-shape" d={sectorPath(RADII.innerFrom, RADII.innerTo, arc, 3)} />
              <text className="option-label" x={label.x} y={label.y}>
                {profileLabel(p)}
              </text>
            </g>
          );
        })}

        {FORMATS.map((f, i) => {
          const arc = FORMAT_ARCS[i];
          const label = polar(INNER_LABEL_R, arcMid(arc));
          const active = f === format;
          return (
            <g
              key={f}
              data-hit
              className={`wedge wedge-option wedge-format${active ? " is-active" : ""}`}
              role="menuitemradio"
              aria-checked={active}
              aria-label={`Format: ${formatLabel(f)}`}
              onClick={() => setFormat(f)}
            >
              <path className="wedge-shape" d={sectorPath(RADII.innerFrom, RADII.innerTo, arc, 3)} />
              <text className="option-label option-label-sm" x={label.x} y={label.y}>
                {formatLabel(f)}
              </text>
            </g>
          );
        })}

        <g data-hit className="center" onClick={() => highlight >= 0 && select(highlight)}>
          <circle className="center-disc" cx={CENTER} cy={CENTER} r={RADII.center} />
          <text className="center-summary" x={CENTER} y={CENTER - 18}>
            {session.summary}
          </text>
          <text className="center-detail" x={CENTER} y={CENTER + 3}>
            {centerDetail}
          </text>
          <text className={`center-hint${notice ? " is-warning" : ""}`} x={CENTER} y={CENTER + 24}>
            {notice ?? "Esc to close · Tab ↻"}
          </text>
          {session.unreadable.length > 0 && (
            <text className="center-hint is-warning" x={CENTER} y={CENTER + 40}>
              {session.unreadable.length} unreadable
            </text>
          )}
        </g>
      </svg>
      <div className="visually-hidden" aria-live="polite">
        {hovered ? `${hovered.preset}, ${centerDetail}` : session.summary}
      </div>
      {error && (
        <div className="radial-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
