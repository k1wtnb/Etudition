import { describe, expect, it } from "vitest";
import type { MarkView } from "./bindings/MarkView";
import {
  anchorOf,
  beatSymbol,
  cursorStepAt,
  formatScore,
  labelY,
  liftToClear,
  type MarkKind,
  markKind,
  markStyle,
  noteName,
  quartersAt,
  quarterTempo,
  tempoInBeats,
} from "./present";

describe("印の種類と形（AC-18 の AI の部分）", () => {
  const mark = (p: MarkView["pitch"], t: MarkView["timing"]): MarkView => ({
    ref_index: 0,
    pitch: p,
    timing: t,
  });

  it("判定から印の種類を決める", () => {
    expect(markKind(mark("correct", "within"))).toBe("ok");
    expect(markKind(mark("correct", "not_judged"))).toBe("ok");
    expect(markKind(mark("correct", "early"))).toBe("early");
    expect(markKind(mark("correct", "late"))).toBe("late");
    expect(markKind(mark("wrong_pitch", "within"))).toBe("wrong");
    expect(markKind(mark("wrong_pitch", "late"))).toBe("wrong");
    expect(markKind(mark("missed", "not_judged"))).toBe("missed");
  });

  it("印の種類ごとに、形も色も文字も違う（色だけに頼らない。REQ-090）", () => {
    const kinds: MarkKind[] = [
      "ok",
      "early",
      "late",
      "wrong",
      "missed",
      "length",
    ];
    const styles = kinds.map(markStyle);
    expect(new Set(styles.map((s) => s.shape)).size).toBe(kinds.length);
    expect(new Set(styles.map((s) => s.color)).size).toBe(kinds.length);
    expect(new Set(styles.map((s) => s.label)).size).toBe(kinds.length);
  });
});

describe("点数の表示（SR-17）", () => {
  it("整数部と、小数点以下4桁（0 で埋める）", () => {
    expect(formatScore(666_666)).toBe("66.6666");
    expect(formatScore(900_000)).toBe("90.0000");
    expect(formatScore(1_000_000)).toBe("100.0000");
    expect(formatScore(0)).toBe("0.0000");
    expect(formatScore(5)).toBe("0.0005");
  });
});

describe("カーソルの位置", () => {
  const tempo = { num: 60, den: 1 }; // 四分音符／分
  const origin = { ns: 4_000_000_000 }; // 前カウント 4 拍

  it("時刻を楽譜の位置（四分音符の数）にする", () => {
    expect(quartersAt({ ns: 4_000_000_000 }, origin, tempo)).toBe(0);
    expect(quartersAt({ ns: 5_500_000_000 }, origin, tempo)).toBe(1.5);
    expect(quartersAt({ ns: 3_000_000_000 }, origin, tempo)).toBe(-1);
    // 120 BPM なら倍の速さ
    expect(
      quartersAt({ ns: 5_000_000_000 }, origin, { num: 120, den: 1 }),
    ).toBe(2);
    // 分数のテンポ（135/2 ＝ 67.5）
    expect(
      quartersAt({ ns: 4_000_000_000 + 8_000_000_000 / 9 }, origin, {
        num: 135,
        den: 2,
      }),
    ).toBeCloseTo(1, 9);
  });

  // お手本の音の位置と、その音を置く歩（位置の順）
  const entries = [
    { start: -1, step: 0 }, // 弱起
    { start: 0, step: 1 },
    { start: 0, step: 1 }, // 和音
    { start: 1, step: 2 },
    { start: 2, step: -1 }, // 歩に解決できない音は飛ばす
    { start: 4, step: 5 }, // 繰り返しで前の歩に戻ることもある
    { start: 5, step: 1 },
  ];

  it("その位置までに始まった最後の音の歩", () => {
    expect(cursorStepAt(entries, -1)).toBe(0);
    expect(cursorStepAt(entries, -0.5)).toBe(0);
    expect(cursorStepAt(entries, 0)).toBe(1);
    expect(cursorStepAt(entries, 1.99)).toBe(2);
    expect(cursorStepAt(entries, 3)).toBe(2);
    expect(cursorStepAt(entries, 4)).toBe(5);
    expect(cursorStepAt(entries, 9)).toBe(1);
  });

  it("最初の音より前（前カウントの間）は、最初の音の歩", () => {
    expect(cursorStepAt(entries, -4)).toBe(0);
  });

  it("歩に解決できる音がなければ -1", () => {
    expect(cursorStepAt([], 0)).toBe(-1);
    expect(cursorStepAt([{ start: 0, step: -1 }], 1)).toBe(-1);
  });
});

describe("音名", () => {
  it("MIDI のノート番号を音名にする（中央の C ＝ C4 ＝ 60）", () => {
    expect(noteName(60)).toBe("C4");
    expect(noteName(61)).toBe("C♯4");
    expect(noteName(69)).toBe("A4");
    expect(noteName(21)).toBe("A0");
    expect(noteName(0)).toBe("C-1");
  });
});

describe("印の文字の高さ", () => {
  it("同じ符頭に2つ目の文字を描くときは、1つ目と重ならないよう1行上に置く（Astra の監査）", () => {
    const top = 100;
    expect(labelY(top, 0)).toBe(91);
    expect(labelY(top, 1)).toBe(78);
    // 1行の高さは文字の大きさ（11）より大きい
    expect(labelY(top, 0) - labelY(top, 1)).toBeGreaterThan(11);
  });
});

describe("テンポの表示（楽譜の慣例。Q-29、人間の回答 2026-11-26）", () => {
  it("音楽の拍の音符の記号", () => {
    expect(beatSymbol({ num: 1, den: 1 })).toBe("♩");
    expect(beatSymbol({ num: 3, den: 2 })).toBe("♩.");
    expect(beatSymbol({ num: 1, den: 2 })).toBe("♪");
    expect(beatSymbol({ num: 2, den: 1 })).toBe("𝅗𝅥");
    expect(beatSymbol({ num: 3, den: 1 })).toBe("𝅗𝅥.");
    expect(beatSymbol({ num: 3, den: 4 })).toBe("♪.");
  });

  it("拍の音符で入れたテンポを、四分音符／分に直して渡す（6/8 の ♩.＝40 は ♩＝60）", () => {
    expect(quarterTempo(40, { num: 3, den: 2 })).toEqual({ num: 120, den: 2 });
    expect(quarterTempo(60, { num: 1, den: 1 })).toEqual({ num: 60, den: 1 });
    expect(quarterTempo(30, { num: 2, den: 1 })).toEqual({ num: 60, den: 1 });
  });

  it("仮の ♩＝60 を、拍の音符の数に直して初めの値にする", () => {
    expect(tempoInBeats(60, { num: 3, den: 2 })).toBe(40);
    expect(tempoInBeats(60, { num: 2, den: 1 })).toBe(30);
    expect(tempoInBeats(60, { num: 1, den: 2 })).toBe(120);
    expect(tempoInBeats(60, { num: 3, den: 4 })).toBe(80);
  });
});

describe("カーソルをクリックに沿わせる（AC-17。1a-5）", () => {
  it("クリックを受けた画面の時刻から、そのクリックの予定時刻を引いた時刻を、時刻 0 とみなす", () => {
    // 画面の 5000 ms に、予定時刻 1.5 秒のクリックを受けた → 時刻 0 は 3500 ms
    expect(anchorOf(5000, { ns: 1_500_000_000 })).toBe(3500);
    // 前カウントの最初（0 秒）のクリックなら、受けた時刻がそのまま時刻 0
    expect(anchorOf(1234, { ns: 0 })).toBe(1234);
  });
});

describe("印の文字の置き場所（重ならない。人間の指摘、2026-11-28）", () => {
  const box = (x: number, y: number, width = 20, height = 11) => ({
    x,
    y,
    width,
    height,
  });

  it("何とも重ならなければ動かさない", () => {
    expect(liftToClear(box(0, 50), [box(30, 50), box(0, 80)])).toBe(0);
  });

  it("ほかの文字や音符と重なれば、重ならなくなるまで1行ずつ上へ上げる", () => {
    // 同じ位置の文字が1つ → 1行上へ
    expect(liftToClear(box(0, 50), [box(5, 50)])).toBe(13);
    // 1行上にも文字がある → 2行上へ
    expect(liftToClear(box(0, 50), [box(5, 50), box(5, 37)])).toBe(26);
    // 長い文字は、横に並ぶ音符の箱（符尾を含む）とも重ならないよう上げる
    expect(liftToClear(box(0, 50, 80), [box(60, 30, 10, 40)])).toBe(39);
  });

  it("辺が接するだけなら重なりとみなさない", () => {
    expect(liftToClear(box(0, 50), [box(20, 50), box(0, 61)])).toBe(0);
  });

  it("上げる回数には上限がある（上がりすぎない）", () => {
    const wall = box(0, -10000, 20, 20000);
    expect(liftToClear(box(0, 50), [wall], 13, 5)).toBe(65);
  });
});
