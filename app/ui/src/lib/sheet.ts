// 練習の画面の楽譜：OSMD で横1段に描き、お手本の音と OSMD の音符を突き合わせ（S1 の2段の対応）、
// カーソルと印を自分の SVG の層に直接描く（Svelte の状態を通さない。仕様書8章、ADR-0001）。
import { type Note, OpenSheetMusicDisplay } from "opensheetmusicdisplay";
import type { ScoreView } from "./bindings/ScoreView";
import { type CursorStep, checkSounding, matchWritten } from "./match";
import {
  drawMark,
  extractNotes,
  moveCursor,
  noteBoxes,
  noteheadOf,
  resetMarkLayer,
  walkCursor,
} from "./osmd";
import {
  type Box,
  type CursorEntry,
  cursorStepAt,
  type MarkStyle,
} from "./present";

/** 突き合わせの数（画面に出して、対応が取れたかを見る）。 */
export type SheetSummary = {
  written: number;
  writtenMatched: number;
  refs: number;
  refsResolved: number;
  unresolved: { index: number; reason: string }[];
};

export class Sheet {
  private osmd: OpenSheetMusicDisplay;
  private notes: Note[] = [];
  private byCoreId = new Map<string, number>();
  private score: ScoreView | undefined;
  private entries: CursorEntry[] = [];
  private layer: SVGGElement | null = null;
  private at = -1;
  /** OSMD の音符ごとの、いま描いてある印（繰り返しで同じ符頭を通ったら置き換える）。 */
  private drawn = new Map<number, SVGElement[]>();
  /** 楽譜の音符の箱（印の文字を重ねないため。描き直したときに集め直す）。 */
  private boxes: Box[] = [];

  constructor(private container: HTMLElement) {
    this.osmd = new OpenSheetMusicDisplay(container, {
      backend: "svg",
      autoResize: false,
      drawTitle: false,
      renderSingleHorizontalStaffline: true,
    });
  }

  /** 楽譜を描き、突き合わせる。 */
  async load(score: ScoreView): Promise<SheetSummary> {
    this.score = score;
    await this.osmd.load(score.xml);
    this.osmd.render();
    const { notes, info } = extractNotes(this.osmd);
    this.notes = notes;
    const match = matchWritten(score.written, info);
    this.byCoreId = match.byCoreId;
    const walk = walkCursor(this.osmd);
    const indexOf = new Map(notes.map((n, i) => [n, i]));
    const steps: CursorStep[] = walk.steps.map((s) => ({
      measureIndex: s.measureIndex,
      visit: s.visit,
      notes: new Set(
        [...s.notes].flatMap((n) => {
          const i = indexOf.get(n);
          return i === undefined ? [] : [i];
        }),
      ),
    }));
    const measureOf = new Map(
      score.written.map((w) => [w.id, w.measure_index]),
    );
    const check = checkSounding(
      score.notes,
      match.byCoreId,
      measureOf,
      steps,
      (o) => noteheadOf(this.osmd, notes[o]) !== undefined,
    );
    this.entries = score.notes
      .map((r, i) => ({
        start: r.start_quarters.num / r.start_quarters.den,
        step: check.stepOf[i] ?? -1,
      }))
      .sort((a, b) => a.start - b.start);
    this.boxes = noteBoxes(this.container);
    this.clearMarks();
    this.at = -1;
    this.showCursorAt(Number.NEGATIVE_INFINITY);
    return {
      written: score.written.length,
      writtenMatched: match.byCoreId.size,
      refs: score.notes.length,
      refsResolved: check.resolved,
      unresolved: check.unresolved,
    };
  }

  /** 楽譜の位置 q（四分音符の数）にカーソルを置く。 */
  showCursorAt(q: number): void {
    const step = cursorStepAt(this.entries, q);
    if (step < 0 || step === this.at) return;
    this.at = moveCursor(this.osmd, this.at, step);
  }

  /** 印を消す。 */
  clearMarks(): void {
    this.layer = resetMarkLayer(this.container);
    this.drawn.clear();
  }

  /**
   * お手本の ref_index の音に印を描く。`replace` なら同じ符頭の前の印は消す（繰り返しの2回目で重ねない）。
   * `replace` でなければ前の印を残す（止まった位置の印を、間違えた音の印に重ねて描く）。文字はほかの文字や音符と重ならない所に置く。
   * 描けなければ false。
   */
  mark(
    refIndex: number,
    style: MarkStyle,
    extra = "",
    replace = true,
  ): boolean {
    const ref = this.score?.notes[refIndex];
    const w = ref?.written[0];
    const o = w === undefined ? undefined : this.byCoreId.get(w.id);
    if (!this.layer || w === undefined || o === undefined) return false;
    const pass = w.pass > 1 ? `(${w.pass}回目)` : "";
    if (!replace) {
      const label = `${style.label}${pass}${extra}`;
      return (
        drawMark(
          this.osmd,
          this.layer,
          this.notes[o],
          style,
          label,
          this.boxes,
        ) !== undefined
      );
    }
    for (const e of this.drawn.get(o) ?? []) e.remove();
    const drawn = drawMark(
      this.osmd,
      this.layer,
      this.notes[o],
      style,
      `${style.label}${pass}${extra}`,
      this.boxes,
    );
    if (drawn) this.drawn.set(o, drawn);
    else this.drawn.delete(o);
    return drawn !== undefined;
  }
}
