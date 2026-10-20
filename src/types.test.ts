import { describe, it, expect } from "vitest";
import { parseShorthand } from "./types";

describe("parseShorthand", () => {
  it("parses decision shorthand correctly", () => {
    const result = parseShorthand("dec: Approved budget");
    expect(result).toEqual({
      kind: "decision",
      text: "Approved budget",
    });
  });

  it("parses action shorthand with owner, task, and due date", () => {
    const result = parseShorthand("act: Sarah Deploy fix Tomorrow");
    expect(result).toEqual({
      kind: "action",
      text: "Deploy fix",
      owner: "Sarah",
      due: "Tomorrow",
    });
  });

  it("parses risk shorthand correctly", () => {
    const result = parseShorthand("risk: SOC2 latency");
    expect(result).toEqual({
      kind: "risk",
      text: "SOC2 latency",
    });
  });

  it("falls back to plain discussion on standard text", () => {
    const result = parseShorthand("Discussing roadmap for Q3");
    expect(result).toEqual({
      kind: "discussion",
      text: "Discussing roadmap for Q3",
    });
  });
});
