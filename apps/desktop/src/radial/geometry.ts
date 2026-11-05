// Geometry for the radial (donut) menu. Angles are degrees, 0 = 12 o'clock,
// increasing clockwise (matching how the menu is read).

export const VIEW = 420;
export const CENTER = VIEW / 2;

/** Ring radii in view-box units (1 unit = 1 CSS px at 100%). */
export const RADII = {
  disc: 198,
  center: 64,
  innerFrom: 70,
  innerTo: 118,
  outerFrom: 124,
  outerTo: 194,
} as const;

export interface Arc {
  start: number;
  end: number;
}

export interface Point {
  x: number;
  y: number;
}

export function polar(radius: number, deg: number, cx = CENTER, cy = CENTER): Point {
  const rad = (deg * Math.PI) / 180;
  return { x: cx + radius * Math.sin(rad), y: cy - radius * Math.cos(rad) };
}

export function arcMid(arc: Arc): number {
  return (arc.start + arc.end) / 2;
}

/** Split [from, to] into `count` equal arcs. */
export function evenArcs(count: number, from: number, to: number): Arc[] {
  const step = (to - from) / count;
  return Array.from({ length: count }, (_, i) => ({ start: from + i * step, end: from + (i + 1) * step }));
}

/** Quality wedges: five 72° wedges, the first centred at 12 o'clock (2K → 16K clockwise). */
export const OUTER_ARCS: Arc[] = evenArcs(5, -36, 324);
/** Model profile: upper half (Photo left, Anime right). */
export const PROFILE_ARCS: Arc[] = evenArcs(2, -90, 90);
/** Output format: lower half, clockwise from 3 o'clock (Keep, PNG, JPG, WEBP). */
export const FORMAT_ARCS: Arc[] = evenArcs(4, 90, 270);

const fmt = (n: number) => (Math.round(n * 100) / 100).toString();

/**
 * SVG path for an annular sector between radii r0 < r1, with a constant
 * `gap` (in px) carved out on both sides so neighbouring wedges separate cleanly.
 */
export function sectorPath(r0: number, r1: number, arc: Arc, gap = 0): string {
  const toDeg = (px: number, r: number) => ((px / 2 / r) * 180) / Math.PI;
  const outerPad = gap > 0 ? toDeg(gap, r1) : 0;
  const innerPad = gap > 0 ? toDeg(gap, r0) : 0;
  const a = polar(r1, arc.start + outerPad);
  const b = polar(r1, arc.end - outerPad);
  const c = polar(r0, arc.end - innerPad);
  const d = polar(r0, arc.start + innerPad);
  const large = arc.end - arc.start > 180 ? 1 : 0;
  return [
    `M ${fmt(a.x)} ${fmt(a.y)}`,
    `A ${r1} ${r1} 0 ${large} 1 ${fmt(b.x)} ${fmt(b.y)}`,
    `L ${fmt(c.x)} ${fmt(c.y)}`,
    `A ${r0} ${r0} 0 ${large} 0 ${fmt(d.x)} ${fmt(d.y)}`,
    "Z",
  ].join(" ");
}

/** Full ring (annulus) path, used as a backdrop band. */
export function ringPath(r0: number, r1: number): string {
  const ring = (r: number, sweep: 0 | 1) =>
    `M ${CENTER} ${CENTER - r} A ${r} ${r} 0 1 ${sweep} ${CENTER} ${CENTER + r} A ${r} ${r} 0 1 ${sweep} ${CENTER} ${CENTER - r} Z`;
  return `${ring(r1, 1)} ${ring(r0, 0)}`;
}

/** Distance of a point from the menu centre. */
export function distanceFromCenter(p: Point): number {
  return Math.hypot(p.x - CENTER, p.y - CENTER);
}
