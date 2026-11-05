import { describe, expect, it } from "vitest";
import { cycle, keyToAction, stepEnabled } from "./keyboard";

describe("keyToAction", () => {
  it("maps the documented hotkeys", () => {
    expect(keyToAction("2", false)).toEqual({ type: "select", preset: "2K" });
    expect(keyToAction("4", false)).toEqual({ type: "select", preset: "4K" });
    expect(keyToAction("8", false)).toEqual({ type: "select", preset: "8K" });
    expect(keyToAction("1", false)).toEqual({ type: "select", preset: "12K" });
    expect(keyToAction("6", false)).toEqual({ type: "select", preset: "16K" });
  });

  it("maps navigation, confirm, cancel and option cycling", () => {
    expect(keyToAction("ArrowRight", false)).toEqual({ type: "move", delta: 1 });
    expect(keyToAction("ArrowUp", false)).toEqual({ type: "move", delta: -1 });
    expect(keyToAction("Enter", false)).toEqual({ type: "confirm" });
    expect(keyToAction("Escape", false)).toEqual({ type: "cancel" });
    expect(keyToAction("Tab", false)).toEqual({ type: "cycleProfile", delta: 1 });
    expect(keyToAction("Tab", true)).toEqual({ type: "cycleProfile", delta: -1 });
    expect(keyToAction("f", false)).toEqual({ type: "cycleFormat", delta: 1 });
    expect(keyToAction("x", false)).toBeNull();
  });
});

describe("stepEnabled", () => {
  const enabled = [false, true, true, false, true];

  it("skips disabled entries and wraps", () => {
    expect(stepEnabled(1, 1, enabled)).toBe(2);
    expect(stepEnabled(2, 1, enabled)).toBe(4);
    expect(stepEnabled(4, 1, enabled)).toBe(1);
    expect(stepEnabled(1, -1, enabled)).toBe(4);
  });

  it("starts from either end when nothing is highlighted", () => {
    expect(stepEnabled(-1, 1, enabled)).toBe(1);
    expect(stepEnabled(-1, -1, enabled)).toBe(4);
  });

  it("returns -1 when nothing is enabled", () => {
    expect(stepEnabled(0, 1, [false, false])).toBe(-1);
  });
});

describe("cycle", () => {
  it("wraps in both directions", () => {
    expect(cycle(["a", "b", "c"], "c", 1)).toBe("a");
    expect(cycle(["a", "b", "c"], "a", -1)).toBe("c");
  });
});
