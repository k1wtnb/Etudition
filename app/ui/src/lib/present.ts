// 画面に出す形を作る純関数（OSMD と DOM に触らない）。時刻は表示だけに使い、採点には渡さない（CLAUDE.md の7章）。
import type { MarkView } from "./bindings/MarkView";
import type { TempoView } from "./bindings/TempoView";
import type { UiTime } from "./bindings/UiTime";

/** 印の種類。`length`（音の長さの誤り）は演奏後の判定にだけ出る。 */
export type MarkKind = "ok" | "early" | "late" | "wrong" | "missed" | "length";

/** 判定から印の種類を決める。音程が違えば、タイミングによらず「違う音」。 */
export function markKind(m: Pick<MarkView, "pitch" | "timing">): MarkKind {
  if (m.pitch === "missed") return "missed";
  if (m.pitch === "wrong_pitch") return "wrong";
  if (m.timing === "early") return "early";
  if (m.timing === "late") return "late";
  return "ok";
}

/** 印の形。 */
export type MarkShape =
  | "circle"
  | "tri-left"
  | "tri-right"
  | "cross"
  | "dash"
  | "underline";

/** 印の見た目（色だけに頼らず、形と文字も変える。REQ-090、I05）。 */
export type MarkStyle = { shape: MarkShape; color: string; label: string };

const STYLES: Record<MarkKind, MarkStyle> = {
  ok: { shape: "circle", color: "#1a7f37", label: "○" },
  early: { shape: "tri-left", color: "#9a6700", label: "早" },
  late: { shape: "tri-right", color: "#8250df", label: "遅" },
  wrong: { shape: "cross", color: "#cf222e", label: "×" },
  missed: { shape: "dash", color: "#57606a", label: "抜" },
  length: { shape: "underline", color: "#bc4c00", label: "長さ" },
};

/** 印の種類の見た目。 */
export function markStyle(kind: MarkKind): MarkStyle {
  return STYLES[kind];
}

/** 100 点にあたる q（採点の仕様 v0 の `Q_PER_POINT` × 100）の、1点あたりの q。 */
const Q_PER_POINT = 10_000;

/** 点数の表示（SR-17）：整数部と、小数点以下4桁（0 で埋める）。 */
export function formatScore(q: number): string {
  const whole = Math.floor(q / Q_PER_POINT);
  const frac = String(q % Q_PER_POINT).padStart(4, "0");
  return `${whole}.${frac}`;
}

/** 時刻を、楽譜の位置（四分音符の数。1小節目の頭が 0）にする。表示だけに使う。 */
export function quartersAt(
  at: UiTime,
  origin: UiTime,
  tempo: TempoView,
): number {
  return ((at.ns - origin.ns) * tempo.num) / (tempo.den * 60e9);
}

/** お手本の1音の位置（四分音符の数）と、カーソルを置く歩（解決できなければ -1）。位置の順に並べる。 */
export type CursorEntry = { start: number; step: number };

/**
 * 楽譜の位置 q でカーソルを置く歩：q までに始まった、歩に解決できた最後の音の歩。
 * q が最初の音より前（前カウントの間）なら最初の音の歩。解決できた音がなければ -1。
 */
export function cursorStepAt(entries: CursorEntry[], q: number): number {
  let step = -1;
  for (const e of entries) {
    if (e.step < 0) continue;
    if (e.start > q && step >= 0) break;
    step = e.step;
    if (e.start > q) break;
  }
  return step;
}

const NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];

/** MIDI のノート番号を音名にする（中央の C ＝ C4 ＝ 60）。 */
export function noteName(note: number): string {
  return `${NAMES[note % 12]}${Math.floor(note / 12) - 1}`;
}
