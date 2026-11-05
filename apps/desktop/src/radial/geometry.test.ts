import { describe, expect, it } from "vitest";
import {
  CENTER,
  FORMAT_ARCS,
  OUTER_ARCS,
  PROFILE_ARCS,
  arcMid,
  distanceFromCenter,
  evenArcs,
  polar,
  ringPath,
  sectorPath,
} from "./geometry";

describe("polar", () => {
  it("puts 0° at 12 o'clock and goes clockwise", () => {
    const top = polar(100, 0);
    expect(top.x).toBeCloseTo(CENTER);
    expect(top.y).toBeCloseTo(CENTER - 100);
    const right = polar(100, 90);
    expect(right.x).toBeCloseTo(CENTER + 100);
    expect(right.y).toBeCloseTo(CENTER);
  });
});

describe("arcs", () => {
  it("splits ranges evenly", () => {
    expect(evenArcs(4, 90, 270)).toEqual([
      { start: 90, end: 135 },
      { start: 135, end: 180 },
      { start: 180, end: 225 },
      { start: 225, end: 270 },
    ]);
  });

  it("centres the first quality wedge at the top and covers the full circle", () => {
    expect(OUTER_ARCS).toHaveLength(5);
    expect(arcMid(OUTER_ARCS[0])).toBeCloseTo(0);
    expect(OUTER_ARCS[4].end - OUTER_ARCS[0].start).toBeCloseTo(360);
  });

  it("splits the inner ring into profile (top) and format (bottom) halves", () => {
    expect(PROFILE_ARCS[0]).toEqual({ start: -90, end: 0 });
    expect(PROFILE_ARCS[1]).toEqual({ start: 0, end: 90 });
    expect(FORMAT_ARCS[0].start).toBe(90);
    expect(FORMAT_ARCS[3].end).toBe(270);
  });
});

describe("paths", () => {
  it("draws a closed sector with two arcs", () => {
    const d = sectorPath(70, 118, { start: 0, end: 90 }, 4);
    expect(d.startsWith("M ")).toBe(true);
    expect(d.match(/A /g)).toHaveLength(2);
    expect(d.endsWith("Z")).toBe(true);
    expect(d).not.toContain("NaN");
  });

  it("uses the large-arc flag only for sectors over 180°", () => {
    expect(sectorPath(10, 20, { start: 0, end: 72 })).toContain(" 0 0 1 ");
    expect(sectorPath(10, 20, { start: 0, end: 200 })).toContain(" 0 1 1 ");
  });

  it("draws rings as two circles", () => {
    expect(ringPath(70, 118).match(/M /g)).toHaveLength(2);
  });

  it("measures distance from the centre", () => {
    expect(distanceFromCenter({ x: CENTER + 3, y: CENTER + 4 })).toBe(5);
  });
});
