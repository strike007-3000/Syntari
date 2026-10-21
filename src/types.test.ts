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

  it("parses action with ISO date via 'by' keyword", () => {
    const result = parseShorthand("act: Alice Migrate DB by 2025-01-31");
    expect(result).toEqual({ kind: "action", text: "Migrate DB", owner: "Alice", due: "2025-01-31" });
  });

  it("parses action with slash date via 'on' keyword", () => {
    const result = parseShorthand("act: Bob Fix bug on 12/31");
    expect(result).toEqual({ kind: "action", text: "Fix bug", owner: "Bob", due: "12/31" });
  });

  it("parses action with day-of-week due", () => {
    const result = parseShorthand("act: Dana staging fix Friday");
    expect(result).toEqual({ kind: "action", text: "staging fix", owner: "Dana", due: "Friday" });
  });

  it("parses action with no due date", () => {
    const result = parseShorthand("act: Eve Review PR");
    expect(result).toEqual({ kind: "action", text: "Review PR", owner: "Eve", due: undefined });
  });

  it("is case-insensitive for prefix", () => {
    expect(parseShorthand("DEC: Upper case").kind).toBe("decision");
    expect(parseShorthand("ACT: Frank Do thing").kind).toBe("action");
    expect(parseShorthand("RISK: Blocker").kind).toBe("risk");
  });

  it("trims whitespace", () => {
    const result = parseShorthand("  dec:  trimmed  ");
    expect(result).toEqual({ kind: "decision", text: "trimmed" });
  });
});
