import { it, expect, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useStore, pollInsights, clockNow } from "./store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-sql", () => ({ default: { load: vi.fn() } }));

it("drops a queued insight poll once another meeting is open", async () => {
  useStore.setState({ sessionId: 1 });
  const queued = pollInsights();
  useStore.setState({ sessionId: 2 });
  await queued;
  expect(invoke).not.toHaveBeenCalled();
});

it("freezes the meeting clock while paused and does not count the pause on resume", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(10_000);
  useStore.setState({ running: true, paused: false, pausedAt: null, startedAt: 4_000 });
  await useStore.getState().pause();
  vi.setSystemTime(70_000);
  expect(clockNow(useStore.getState()) - useStore.getState().startedAt).toBe(6_000); // frozen at the pause
  await useStore.getState().resume();
  const s = useStore.getState();
  expect(s.paused).toBe(false);
  expect(clockNow(s) - s.startedAt).toBe(6_000); // the 60 s pause is gone
  vi.setSystemTime(75_000);
  expect(clockNow(useStore.getState()) - useStore.getState().startedAt).toBe(11_000);
  vi.useRealTimers();
});
