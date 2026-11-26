// 回の終わり方ごとに画面に出すもの（仕様書 10-1 の表）。純関数（DOM と OSMD に触らない）。
import type { LiveEvent } from "./bindings/LiveEvent";
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
      recordError: string | null;
    }
  | {
      kind: "stopped";
      /** 止まった位置の組の先頭の ref_index。 */
      stoppedAt: number | null;
      /** 止まった位置までの間違えた音（W03）。 */
      mistakes: { refIndex: number; kind: MarkKind }[];
      recordError: string | null;
    }
  | { kind: "invalid"; reason: string; recordError: string | null };

function recordError(r: RecordView): string | null {
  return r.status === "failed" ? r.message : null;
}

/** 終わりの通知から、画面に出すものを作る。終わりの通知でなければ null。 */
export function outcomeOf(ev: LiveEvent): Outcome | null {
  if (ev.type === "invalid") {
    // Q-52（無効の回の画面）は未確定。理由の文字だけを出す（仮）
    return {
      kind: "invalid",
      reason: ev.reason,
      recordError: recordError(ev.record),
    };
  }
  if (ev.type !== "ended") return null;
  const r = ev.result;
  if (r.end === "stopped" || r.items === null || r.passed === null) {
    return {
      kind: "stopped",
      stoppedAt: r.stopped_at,
      mistakes: r.notes
        .map((n) => ({ refIndex: n.ref_index, kind: markKind(n) }))
        .filter((m) => m.kind !== "ok"),
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
    recordError: recordError(ev.record),
  };
}
