import { describe, expect, it } from "vitest";
import {
  type CoreExpanded,
  type CoreSounding,
  type CoreWritten,
  type CursorStep,
  checkSounding,
  compareWalk,
  matchWritten,
  type OsmdNote,
} from "./match";

const q = (num: number, den = 1) => ({ num, den });

const core = (
  id: string,
  measure_index: number,
  voice: string,
  staff: number,
  pitch: number,
  offset: { num: number; den: number },
  is_grace = false,
): CoreWritten => ({
  id,
  measure_index,
  voice,
  staff,
  pitch,
  offset_quarters: offset,
  is_grace,
});

const osmd = (
  measureIndex: number,
  voice: number,
  staff: number,
  pitch: number,
  offsetQuarters: { num: number; den: number },
  isGrace = false,
): OsmdNote => ({ measureIndex, voice, staff, pitch, offsetQuarters, isGrace });

describe("1段目：書かれた音符と OSMD の音符", () => {
  it("内容のキーで突き合わせ、並びの順には頼らない", () => {
    const r = matchWritten(
      [
        core("a", 0, "1", 1, 60, q(0)),
        core("b", 0, "1", 1, 64, q(0)),
        core("c", 0, "5", 2, 48, q(1, 3)),
      ],
      [
        osmd(0, 5, 2, 48, q(2, 6)),
        osmd(0, 1, 1, 64, q(0)),
        osmd(0, 1, 1, 60, q(0)),
      ],
    );
    expect([...r.byCoreId]).toEqual([
      ["a", 2],
      ["b", 1],
      ["c", 0],
    ]);
    expect(r.coreMissing).toEqual([]);
    expect(r.coreAmbiguous).toEqual([]);
    expect(r.osmdUnmatched).toEqual([]);
  });

  it("相手のない音符と、2つ以上に当たる音符を一覧に出す", () => {
    const r = matchWritten(
      [
        core("a", 0, "1", 1, 60, q(0)),
        core("g", 0, "1", 1, 62, q(0), true),
        core("x", 1, "1", 1, 67, q(0)),
      ],
      [
        osmd(0, 1, 1, 60, q(0)),
        osmd(0, 1, 1, 62, q(0), false),
        osmd(1, 1, 1, 67, q(0)),
        osmd(1, 1, 1, 67, q(0)),
      ],
    );
    expect([...r.byCoreId]).toEqual([["a", 0]]);
    expect(r.coreMissing).toEqual(["g"]);
    expect(r.coreAmbiguous).toEqual(["x"]);
    expect(r.osmdUnmatched).toEqual([1, 2, 3]);
  });
});

describe("2段目：展開の順と OSMD のカーソルの歩み", () => {
  const expanded: CoreExpanded[] = [
    { measure_index: 0, pass: 1, start_quarters: q(0) },
    { measure_index: 1, pass: 1, start_quarters: q(4) },
    { measure_index: 0, pass: 2, start_quarters: q(8) },
  ];

  it("小節の列が同じなら一致", () => {
    expect(compareWalk(expanded, [0, 1, 0]).firstMismatch).toBeNull();
  });

  it("最初に食い違う歩を返す", () => {
    expect(compareWalk(expanded, [0, 1, 2]).firstMismatch).toBe(2);
    expect(compareWalk(expanded, [0, 1]).firstMismatch).toBe(2);
  });

  it("鳴る音ごとに、対象の音を含む歩に解決し、印を描けるかを確かめる", () => {
    const sounding: Pick<CoreSounding, "written">[] = [
      { written: [{ id: "a", pass: 1 }] },
      { written: [{ id: "a", pass: 2 }] },
      { written: [{ id: "a", pass: 3 }] },
      { written: [{ id: "z", pass: 1 }] },
      { written: [{ id: "b", pass: 1 }] },
      { written: [{ id: "c", pass: 1 }] },
    ];
    const byCoreId = new Map([
      ["a", 7],
      ["b", 8],
      ["c", 9],
    ]);
    const measureOf = new Map([
      ["a", 0],
      ["b", 0],
      ["c", 0],
    ]);
    // 小節 0 を2回通る。b（8）はどの歩にもいない。c（9）は1回目の歩にいるが印を描けない
    const steps: CursorStep[] = [
      { measureIndex: 0, visit: 1, notes: new Set([7, 9]) },
      { measureIndex: 1, visit: 1, notes: new Set([]) },
      { measureIndex: 0, visit: 2, notes: new Set([7]) },
    ];
    const r = checkSounding(
      sounding,
      byCoreId,
      measureOf,
      steps,
      (i) => i !== 9,
    );
    expect(r.resolved).toBe(2);
    expect(r.stepOf).toEqual([0, 2, -1, -1, 0, 0]);
    expect(r.fallbackRefs).toBe(1);
    expect(r.markFailures).toBe(1);
    expect(r.unresolved).toEqual([
      { index: 2, reason: "a の 3 回目にカーソルが来ない" },
      { index: 3, reason: "z に OSMD の音符がない" },
      {
        index: 4,
        reason:
          "b の 1 回目は、その音を含む歩がなく代わりの位置（小節の頭）になる",
      },
      { index: 5, reason: "c の印を描けない" },
    ]);
  });
});
