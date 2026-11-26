import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
class FakeChannel<T> {
  onmessage: (ev: T) => void = () => {};
}
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: FakeChannel }));

const { p0TimeProbe, loadScore, startPractice, stopPractice, feedMidiFile } =
  await import("./commands");

// Rust の側（app/src-tauri/src/practice.rs の command_names）と同じ名前で呼ぶことを確かめる（計画 2-3 の3）
describe("Tauri のコマンドの呼び出し", () => {
  beforeEach(() => invoke.mockReset());

  it("p0TimeProbe は Rust の p0_time_probe を引数なしで呼ぶ", async () => {
    invoke.mockResolvedValue({ ns: 9007199254740991 });
    const t = await p0TimeProbe();
    expect(invoke).toHaveBeenCalledWith("p0_time_probe");
    expect(t.ns).toBe(9007199254740991);
    expect(Number.isSafeInteger(t.ns)).toBe(true);
  });

  it("JSON の上限の値は number で正確に読める（2^53 − 1）", () => {
    expect(JSON.parse('{"ns":9007199254740991}').ns).toBe(
      Number.MAX_SAFE_INTEGER,
    );
  });

  it("loadScore は load_score を path で呼ぶ", async () => {
    invoke.mockResolvedValue({ xml: "<x/>", notes: [] });
    const v = await loadScore("曲.musicxml");
    expect(invoke).toHaveBeenCalledWith("load_score", {
      path: "曲.musicxml",
    });
    expect(v.notes).toEqual([]);
  });

  it("startPractice は start_practice を tempo と Channel で呼び、通知を渡す", async () => {
    invoke.mockResolvedValue(undefined);
    const got: unknown[] = [];
    await startPractice({ num: 135, den: 2 }, (ev) => got.push(ev));
    const [name, args] = invoke.mock.calls[0];
    expect(name).toBe("start_practice");
    expect(args.tempo).toEqual({ num: 135, den: 2 });
    args.events.onmessage({ type: "note_received", note: 60, at: { ns: 1 } });
    expect(got).toEqual([{ type: "note_received", note: 60, at: { ns: 1 } }]);
  });

  it("stopPractice は stop_practice を引数なしで呼ぶ", async () => {
    invoke.mockResolvedValue(undefined);
    await stopPractice();
    expect(invoke).toHaveBeenCalledWith("stop_practice");
  });

  it("feedMidiFile は feed_midi_file を path、tempo、Channel で呼ぶ", async () => {
    invoke.mockResolvedValue(undefined);
    await feedMidiFile("誤り.mid", { num: 60, den: 1 }, () => {});
    const [name, args] = invoke.mock.calls[0];
    expect(name).toBe("feed_midi_file");
    expect(args.path).toBe("誤り.mid");
    expect(args.tempo).toEqual({ num: 60, den: 1 });
    expect(args.events).toBeInstanceOf(FakeChannel);
  });
});
