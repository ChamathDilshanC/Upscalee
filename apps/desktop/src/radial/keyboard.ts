import type { Preset } from "../shared/api";

export type RadialAction =
  | { type: "select"; preset: Preset }
  | { type: "move"; delta: 1 | -1 }
  | { type: "confirm" }
  | { type: "cancel" }
  | { type: "cycleProfile"; delta: 1 | -1 }
  | { type: "cycleFormat"; delta: 1 | -1 };

const HOTKEYS: Record<string, Preset> = {
  "2": "2K",
  "4": "4K",
  "8": "8K",
  "1": "12K",
  "6": "16K",
};

/** Map a KeyboardEvent.key to a radial action (null = not ours). */
export function keyToAction(key: string, shift: boolean): RadialAction | null {
  const preset = HOTKEYS[key];
  if (preset) return { type: "select", preset };
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return { type: "move", delta: 1 };
    case "ArrowLeft":
    case "ArrowUp":
      return { type: "move", delta: -1 };
    case "Enter":
    case " ":
      return { type: "confirm" };
    case "Escape":
      return { type: "cancel" };
    case "Tab":
      return { type: "cycleProfile", delta: shift ? -1 : 1 };
    case "f":
    case "F":
      return { type: "cycleFormat", delta: shift ? -1 : 1 };
    default:
      return null;
  }
}

/** Next enabled index when stepping by `delta` (wraps). -1 if nothing is enabled. */
export function stepEnabled(current: number, delta: 1 | -1, enabled: readonly boolean[]): number {
  const n = enabled.length;
  if (!enabled.some(Boolean)) return -1;
  let i = current < 0 ? (delta > 0 ? -1 : 0) : current;
  for (let tries = 0; tries < n; tries++) {
    i = (((i + delta) % n) + n) % n;
    if (enabled[i]) return i;
  }
  return -1;
}

/** Cycle through a list of options. */
export function cycle<T>(options: readonly T[], current: T, delta: 1 | -1): T {
  const i = options.indexOf(current);
  const n = options.length;
  return options[(((i < 0 ? 0 : i) + delta) % n + n) % n];
}
