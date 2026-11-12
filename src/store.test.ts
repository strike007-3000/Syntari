import { it, expect, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useStore, pollInsights } from "./store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-sql", () => ({ default: { load: vi.fn() } }));

it("drops a queued insight poll once another meeting is open", async () => {
  useStore.setState({ sessionId: 1 });
  const queued = pollInsights();
  useStore.setState({ sessionId: 2 });
  await queued;
  expect(invoke).not.toHaveBeenCalled();
});
