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

/** 印の文字の1行の高さ（文字の大きさ 11 より少し大きく）。 */
const LABEL_LINE = 13;

/** 符頭の上端 top から、row 行目（0 が符頭のすぐ上）の文字の基線の y を求める。 */
export function labelY(top: number, row: number): number {
  return top - 9 - row * LABEL_LINE;
}

/** 四分音符の数（分子、分母）。 */
type Beat = { num: number; den: number };

const NOTE_SYMBOLS: [number, string][] = [
  [4, "𝅝"],
  [2, "𝅗𝅥"],
  [1, "♩"],
  [1 / 2, "♪"],
  [1 / 4, "𝅘𝅥𝅯"],
];

/** 音楽の拍の音符の記号（付点は「.」を付ける）。楽譜の慣例でテンポを示すため（Q-29）。 */
export function beatSymbol(beat: Beat): string {
  const q = beat.num / beat.den;
  for (const [v, s] of NOTE_SYMBOLS) {
    if (q === v) return s;
    if (q === v * 1.5) return `${s}.`;
  }
  return `${beat.num}/${beat.den} 四分音符`;
}

/** 拍の音符で入れたテンポ（拍／分）を、四分音符／分（分子、分母）に直す。 */
export function quarterTempo(bpm: number, beat: Beat): TempoView {
  return { num: bpm * beat.num, den: beat.den };
}

/** 四分音符／分のテンポを、拍の音符の数（拍／分。整数に丸める）に直す。 */
export function tempoInBeats(quarterBpm: number, beat: Beat): number {
  return Math.round((quarterBpm * beat.den) / beat.num);
}
