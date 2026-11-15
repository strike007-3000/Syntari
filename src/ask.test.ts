import { it, expect } from "vitest";
import { ftsQuery } from "./ask";

it("keeps the meaningful words of a question", () => {
  expect(ftsQuery("What did we decide about the pricing launch?")).toBe('"decide" OR "pricing" OR "launch"');
});

it("drops duplicates and short words, and gives null when nothing is left", () => {
  expect(ftsQuery("pricing, Pricing, and it is on")).toBe('"pricing"');
  expect(ftsQuery("what is it?")).toBeNull();
});

it("never lets a quote through into the MATCH expression", () => {
  expect(ftsQuery('say "hello" now')).toBe('"hello" OR "now"');
});
