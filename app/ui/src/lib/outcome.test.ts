import { describe, expect, it } from "vitest";
import type { LiveEvent } from "./bindings/LiveEvent";
import type { NoteView } from "./bindings/NoteView";
import type { RunResultView } from "./bindings/RunResultView";
import { outcomeOf } from "./outcome";

const note = (
  ref_index: number,
  pitch: NoteView["pitch"],
  timing: NoteView["timing"],
): NoteView => ({
  ref_index,
  is_grace: false,
  pitch,
  played: null,
  timing,
  delta: null,
  duration: "not_judged",
});

const base: RunResultView = {
  rules_version: "v0",
  end: "normal",
  end_at: { ns: 16_000_000_000 },
  notes: [],
  extras: [],
  items: {
    pitch: { errors: 1, denominator: 3, q: 666_666 },
    timing: { errors: 0, denominator: 3, q: 1_000_000 },
    duration: { errors: 0, denominator: 3, q: 1_000_000 },
  },
  passed: false,
  stopped_at: null,
};

describe("回の終わり方ごとの表示（仕様書 10-1）", () => {
  it("正常に終わった回は、3項目の点数（SR-17）と core の合否", () => {
    const o = outcomeOf({
      type: "ended",
      result: base,
      record: { status: "written" },
    });
    expect(o).toEqual({
      kind: "scored",
      items: [
        { name: "音程", score: "66.6666", errors: 1, of: 3 },
        { name: "タイミング", score: "100.0000", errors: 0, of: 3 },
        { name: "音の長さ", score: "100.0000", errors: 0, of: 3 },
      ],
      passed: false,
      cap: false,
      recordError: null,
    });
  });

  it("上限で終わった回も点数を出す", () => {
    const o = outcomeOf({
      type: "ended",
      result: { ...base, end: "cap", passed: true },
      record: { status: "written" },
    });
    expect(o).toMatchObject({ kind: "scored", cap: true, passed: true });
  });

  it("記録に失敗した回は、点数と「記録できなかった」の知らせを両方出す（AC-42 の (4)）", () => {
    const o = outcomeOf({
      type: "ended",
      result: base,
      record: { status: "failed", message: "書き込めない" },
    });
    expect(o).toMatchObject({ kind: "scored", recordError: "書き込めない" });
  });

  it("止まった回は点数を出さず、止まった位置までの間違えた音を出す（W03、AC-20）", () => {
    const o = outcomeOf({
      type: "ended",
      result: {
        ...base,
        end: "stopped",
        items: null,
        passed: null,
        stopped_at: 3,
        notes: [
          note(0, "correct", "within"),
          note(1, "wrong_pitch", "within"),
          note(2, "correct", "late"),
          note(3, "missed", "not_judged"),
        ],
      },
      record: { status: "not_written" },
    });
    expect(o).toEqual({
      kind: "stopped",
      stoppedAt: 3,
      mistakes: [
        { refIndex: 1, kind: "wrong" },
        { refIndex: 2, kind: "late" },
        { refIndex: 3, kind: "missed" },
      ],
      recordError: null,
    });
  });

  it("無効の回は、理由だけを出す（Q-52 が決まるまでの仮）", () => {
    const o = outcomeOf({
      type: "invalid",
      reason: "MIDI の接続が切れた",
      record: { status: "written" },
    });
    expect(o).toEqual({
      kind: "invalid",
      reason: "MIDI の接続が切れた",
      recordError: null,
    });
  });

  it("終わりでない通知は null", () => {
    const evs: LiveEvent[] = [
      { type: "started", origin: { ns: 0 }, tempo: { num: 60, den: 1 } },
      { type: "note_received", note: 60, at: { ns: 0 } },
      { type: "marks", marks: [] },
    ];
    for (const ev of evs) expect(outcomeOf(ev)).toBeNull();
  });
});
