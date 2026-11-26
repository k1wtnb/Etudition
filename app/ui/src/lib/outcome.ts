// 回の終わり方ごとに画面に出すもの（仕様書 10-1 の表）。純関数（DOM と OSMD に触らない）。
import type { LiveEvent } from "./bindings/LiveEvent";
import type { NoteView } from "./bindings/NoteView";
import type { RecordView } from "./bindings/RecordView";
import { formatScore, type MarkKind, markKind } from "./present";

/** 終わった回の表示。 */
export type Outcome =
  | {
      kind: "scored";
      /** 3項目の点数（SR-17 の形）。 */
      items: { name: string; score: string; errors: number; of: number }[];
      /** 合否（core の判定をそのまま使う）。 */
      passed: boolean;
      cap: boolean;
      /** 演奏後の判定で間違えた音（Q-34 の B。演奏中の仮の印は消して、これを描く）。 */
      mistakes: Mistake[];
      recordError: string | null;
    }
  | {
      kind: "stopped";
      /** 止まった位置の組の先頭の ref_index。 */
      stoppedAt: number | null;
      /** 止まった位置までの間違えた音（W03）。`detail` は印に添える文字。 */
      mistakes: Mistake[];
      /** 余分に弾いた音の数（対応する符頭がないので、楽譜の上には出さない）。 */
      extras: number;
      recordError: string | null;
    }
  | {
      kind: "invalid";
      reason: string;
      /** 次にすることの案内（Q-52 の C。演奏中の仮の印は消す）。 */
      advice: string;
      recordError: string | null;
    };

/** 演奏後の判定で間違えた音。`detail` は印に添える文字。 */
export type Mistake = { refIndex: number; kind: MarkKind; detail: string };

function mistakesOf(notes: NoteView[]): Mistake[] {
  return notes
    .map((n) => ({ refIndex: n.ref_index, ...finalMark(n) }))
    .filter((m) => m.kind !== "ok");
}

/** 演奏後の判定の印：音程、タイミング、音の長さの順に、最初に見つかった誤り。 */
export function finalMark(n: NoteView): { kind: MarkKind; detail: string } {
  const kind = markKind(n);
  if (kind !== "ok") return { kind, detail: "" };
  if (n.duration === "short") return { kind: "length", detail: "（短い）" };
  if (n.duration === "long") return { kind: "length", detail: "（長い）" };
  return { kind: "ok", detail: "" };
}

function recordError(r: RecordView): string | null {
  return r.status === "failed" ? r.message : null;
}

/** 終わりの通知から、画面に出すものを作る。終わりの通知でなければ null。 */
export function outcomeOf(ev: LiveEvent): Outcome | null {
  if (ev.type === "invalid") {
    return {
      kind: "invalid",
      reason: ev.reason,
      advice: ev.advice,
      recordError: recordError(ev.record),
    };
  }
  if (ev.type !== "ended") return null;
  const r = ev.result;
  if (r.end === "stopped" || r.items === null || r.passed === null) {
    return {
      kind: "stopped",
      stoppedAt: r.stopped_at,
      mistakes: mistakesOf(r.notes),
      extras: r.extras.length,
      recordError: recordError(ev.record),
    };
  }
  const item = (
    name: string,
    v: { q: number; errors: number; denominator: number },
  ) => ({
    name,
    score: formatScore(v.q),
    errors: v.errors,
    of: v.denominator,
  });
  return {
    kind: "scored",
    items: [
      item("音程", r.items.pitch),
      item("タイミング", r.items.timing),
      item("音の長さ", r.items.duration),
    ],
    passed: r.passed,
    cap: r.end === "cap",
    mistakes: mistakesOf(r.notes),
    recordError: recordError(ev.record),
  };
}
