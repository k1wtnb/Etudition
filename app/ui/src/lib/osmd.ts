// OSMD に触る部分（薄くする。S1 から移した）。音符を素の値の列にする・カーソルを1周させる・自分の SVG の層に印を描く。
import type {
  Fraction,
  Note,
  OpenSheetMusicDisplay,
  VexFlowGraphicalNote,
} from "opensheetmusicdisplay";
import type { OsmdNote } from "./match";
import type { MarkStyle } from "./present";

/** OSMD の楽譜から、休符でない音符と、その突き合わせ用の値を取り出す。 */
export function extractNotes(osmd: OpenSheetMusicDisplay): {
  notes: Note[];
  info: OsmdNote[];
} {
  const seen = new Set<Note>();
  const notes: Note[] = [];
  const info: OsmdNote[] = [];
  for (const sm of osmd.Sheet.SourceMeasures) {
    for (const vc of sm.VerticalSourceStaffEntryContainers) {
      for (const se of vc.StaffEntries) {
        if (!se) continue;
        const staff = se.ParentStaff;
        const staffNo = staff.ParentInstrument.Staves.indexOf(staff) + 1;
        for (const ve of se.VoiceEntries) {
          for (const n of ve.Notes) {
            if (seen.has(n) || n.isRest() || !n.Pitch) continue;
            seen.add(n);
            const ts = ve.Timestamp;
            notes.push(n);
            info.push({
              measureIndex: sm.measureListIndex,
              staff: staffNo,
              voice: ve.ParentVoice.VoiceId,
              // OSMD の halfTone は C4（MIDI 60）を 48 とする（Pitch の octXmlDiff = 3 から）。
              pitch: n.Pitch.getHalfTone() + 12,
              // Timestamp は全音符の数なので、4 倍して四分音符の数にする
              offsetQuarters: {
                num: ts.GetExpandedNumerator() * 4,
                den: ts.Denominator,
              },
              isGrace: ve.IsGrace,
            });
          }
        }
      }
    }
  }
  return { notes, info };
}

/** カーソルの1歩。何回目にその小節を通っているかと、カーソルの下の音符。 */
export type WalkStep = {
  measureIndex: number;
  visit: number;
  notes: Set<Note>;
};

function before(a: Fraction, b: Fraction): boolean {
  return (
    a.GetExpandedNumerator() * b.Denominator <
    b.GetExpandedNumerator() * a.Denominator
  );
}

/** カーソルを頭から終わりまで動かし、歩みを記録する。最後にカーソルを頭に戻す。 */
export function walkCursor(osmd: OpenSheetMusicDisplay): {
  steps: WalkStep[];
  measures: number[];
} {
  const MAX_STEPS = 100000;
  const c = osmd.cursor;
  c.reset();
  const steps: WalkStep[] = [];
  const measures: number[] = [];
  const visits = new Map<number, number>();
  let prevMeasure = -1;
  let prevTs: Fraction | undefined;
  while (!c.Iterator.EndReached && steps.length < MAX_STEPS) {
    const it = c.Iterator;
    const m = it.CurrentMeasureIndex;
    const ts = it.CurrentSourceTimestamp.clone();
    // 同じ小節でも、楽譜の上の時刻が戻ったら、繰り返しで次の回に入ったとみなす
    if (m !== prevMeasure || (prevTs !== undefined && !before(prevTs, ts))) {
      visits.set(m, (visits.get(m) ?? 0) + 1);
      measures.push(m);
    }
    const notes = new Set<Note>();
    for (const ve of it.CurrentVoiceEntries) {
      for (const n of ve.Notes) notes.add(n);
    }
    steps.push({ measureIndex: m, visit: visits.get(m) ?? 1, notes });
    prevMeasure = m;
    prevTs = ts;
    c.next();
  }
  c.reset();
  return { steps, measures };
}

/** カーソルを i 歩目に置いて見せる。今の歩より先なら進め、前なら頭から進め直す。 */
export function moveCursor(
  osmd: OpenSheetMusicDisplay,
  from: number,
  to: number,
): number {
  const c = osmd.cursor;
  let at = from;
  if (to < at || at < 0) {
    c.reset();
    at = 0;
  }
  for (; at < to; at++) c.next();
  c.show();
  return at;
}

const LAYER_ID = "etudition-marks";
const SVG_NS = "http://www.w3.org/2000/svg";

/** 自分の SVG の層（OSMD の svg の最後の子の g）を作り直して返す。 */
export function resetMarkLayer(container: HTMLElement): SVGGElement | null {
  const svg = container.querySelector("svg");
  if (!svg) return null;
  svg.querySelector(`#${LAYER_ID}`)?.remove();
  const g = document.createElementNS(SVG_NS, "g");
  g.id = LAYER_ID;
  g.setAttribute("pointer-events", "none");
  svg.appendChild(g);
  return g;
}

/**
 * 音符の符頭の SVG 要素を返す。見つからなければ undefined。
 *
 * OSMD 2.3.1 では、和音の音は1つの VexFlow の音符を共有し、getSVGGElement() と getNoteheadSVGs() は
 * 和音全体の要素を返す（配布コードで確かめた）。そこで、和音の音を音高の低い順に並べた順位と、
 * 符頭を画面の下から並べた順位を対応させて、選んだ音の符頭を1つ選ぶ。数が合わなければ undefined。
 */
export function noteheadOf(
  osmd: OpenSheetMusicDisplay,
  note: Note,
): SVGGraphicsElement | undefined {
  const g = osmd.EngravingRules.GNote(note) as VexFlowGraphicalNote | undefined;
  const heads = (g?.getNoteheadSVGs?.() ??
    []) as unknown as SVGGraphicsElement[];
  const chord = note.ParentVoiceEntry.Notes.filter(
    (n) => !n.isRest() && n.Pitch,
  ).sort((a, b) => a.Pitch.getHalfTone() - b.Pitch.getHalfTone());
  if (heads.length === 0 || heads.length !== chord.length) return undefined;
  // 画面の y は下ほど大きい。低い音ほど下にあるので、y の大きい順に並べる
  const byY = [...heads].sort((a, b) => b.getBBox().y - a.getBBox().y);
  return byY[chord.indexOf(note)];
}

function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | number>,
): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

/**
 * 音符の符頭に、印の形（丸・左向きの三角・右向きの三角・×・点線の枠）と文字を描く（REQ-090。色だけに頼らない）。
 * 描いた要素を返す。描けなければ undefined（符頭が見つからない）。
 */
export function drawMark(
  osmd: OpenSheetMusicDisplay,
  layer: SVGGElement,
  note: Note,
  style: MarkStyle,
  label: string,
): SVGElement[] | undefined {
  const el = noteheadOf(osmd, note);
  if (!el) return undefined;
  const b = el.getBBox();
  const pad = 3;
  const cx = b.x + b.width / 2;
  const cy = b.y + b.height / 2;
  const line = { stroke: style.color, "stroke-width": 2, fill: "none" };
  const r = Math.max(b.width, b.height) / 2 + pad;
  const shapes: SVGElement[] = [];
  switch (style.shape) {
    case "circle":
      shapes.push(svg("circle", { cx, cy, r, ...line }));
      break;
    case "tri-left": {
      const x = b.x - pad;
      shapes.push(
        svg("polygon", {
          points: `${x - 8},${cy} ${x},${cy - 5} ${x},${cy + 5}`,
          fill: style.color,
        }),
      );
      break;
    }
    case "tri-right": {
      const x = b.x + b.width + pad;
      shapes.push(
        svg("polygon", {
          points: `${x + 8},${cy} ${x},${cy - 5} ${x},${cy + 5}`,
          fill: style.color,
        }),
      );
      break;
    }
    case "cross":
      shapes.push(
        svg("line", {
          x1: cx - r,
          y1: cy - r,
          x2: cx + r,
          y2: cy + r,
          ...line,
        }),
        svg("line", {
          x1: cx - r,
          y1: cy + r,
          x2: cx + r,
          y2: cy - r,
          ...line,
        }),
      );
      break;
    case "underline":
      shapes.push(
        svg("line", {
          x1: b.x - pad,
          y1: b.y + b.height + pad + 2,
          x2: b.x + b.width + pad,
          y2: b.y + b.height + pad + 2,
          ...line,
          "stroke-width": 3,
        }),
      );
      break;
    case "dash":
      shapes.push(
        svg("rect", {
          x: b.x - pad,
          y: b.y - pad,
          width: b.width + pad * 2,
          height: b.height + pad * 2,
          "stroke-dasharray": "3 2",
          ...line,
        }),
      );
      break;
  }
  const text = svg("text", {
    x: b.x,
    y: b.y - pad - 6,
    fill: style.color,
    "font-size": 11,
  });
  text.textContent = label;
  layer.append(...shapes, text);
  return [...shapes, text];
}
