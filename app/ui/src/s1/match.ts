// S1：cli の JSON（core の書かれた音符・展開・鳴る音）と、OSMD の音符を突き合わせる純関数。
// OSMD には触らない（OSMD から取り出す部分は osmd.ts）。位置の単位は四分音符の数。

/** 整数の分数（四分音符の数）。 */
export type Frac = { num: number; den: number };

/** cli の JSON の書かれた音符。 */
export type CoreWritten = {
  id: string;
  measure_index: number;
  voice: string;
  staff: number;
  pitch: number;
  offset_quarters: Frac;
  is_grace: boolean;
};

/** cli の JSON の展開した小節。 */
export type CoreExpanded = {
  measure_index: number;
  pass: number;
  start_quarters: Frac;
};

/** cli の JSON の鳴る音。 */
export type CoreSounding = {
  pitch: number;
  start_quarters: Frac;
  duration_quarters: Frac;
  is_grace: boolean;
  written: { id: string; pass: number }[];
};

/** cli の JSON（etudition-cli s1）。 */
export type S1Json = {
  part: string;
  written: CoreWritten[];
  expanded_measures: CoreExpanded[];
  sounding: CoreSounding[];
  issues: { measure_index: number | null; element: string; detail: string }[];
};

/** OSMD の音符から取り出した、突き合わせに使う値（休符は含めない）。 */
export type OsmdNote = {
  measureIndex: number;
  staff: number;
  voice: number;
  pitch: number;
  offsetQuarters: Frac;
  isGrace: boolean;
};

function gcd(a: number, b: number): number {
  let x = Math.abs(a);
  let y = Math.abs(b);
  while (y !== 0) [x, y] = [y, x % y];
  return x;
}

function fracKey(f: Frac): string {
  const g = gcd(f.num, f.den) || 1;
  const s = f.den < 0 ? -1 : 1;
  return `${(s * f.num) / g}/${(s * f.den) / g}`;
}

/** 突き合わせのキー：小節・譜表・声部・小節の中の位置・音高・装飾音か。 */
function key(
  measure: number,
  staff: number,
  voice: number,
  offset: Frac,
  pitch: number,
  grace: boolean,
): string {
  return `${measure}|${staff}|${voice}|${fracKey(offset)}|${pitch}|${grace ? "g" : "n"}`;
}

/** 1段目の結果。 */
export type WrittenMatch = {
  /** core の ID → OSMD の音符の添字（ちょうど1つに当たったものだけ）。 */
  byCoreId: Map<string, number>;
  /** OSMD に相手がない core の ID。 */
  coreMissing: string[];
  /** 同じキーの音符が core か OSMD に2つ以上あり、1つに決まらない core の ID。 */
  coreAmbiguous: string[];
  /** core に相手がない OSMD の音符の添字。 */
  osmdUnmatched: number[];
};

/** 書かれた音符と OSMD の音符を、内容のキーで1対1に突き合わせる。並びの順には頼らない。 */
export function matchWritten(
  core: CoreWritten[],
  osmd: OsmdNote[],
): WrittenMatch {
  const osmdByKey = new Map<string, number[]>();
  osmd.forEach((n, i) => {
    const k = key(
      n.measureIndex,
      n.staff,
      n.voice,
      n.offsetQuarters,
      n.pitch,
      n.isGrace,
    );
    osmdByKey.set(k, [...(osmdByKey.get(k) ?? []), i]);
  });
  const coreByKey = new Map<string, string[]>();
  const coreKeys = core.map((n) => {
    const k = key(
      n.measure_index,
      n.staff,
      Number.parseInt(n.voice, 10),
      n.offset_quarters,
      n.pitch,
      n.is_grace,
    );
    coreByKey.set(k, [...(coreByKey.get(k) ?? []), n.id]);
    return k;
  });
  const r: WrittenMatch = {
    byCoreId: new Map(),
    coreMissing: [],
    coreAmbiguous: [],
    osmdUnmatched: [],
  };
  const used = new Set<number>();
  core.forEach((n, i) => {
    const hits = osmdByKey.get(coreKeys[i]) ?? [];
    const same = coreByKey.get(coreKeys[i]) ?? [];
    if (hits.length === 0) {
      r.coreMissing.push(n.id);
    } else if (hits.length > 1 || same.length > 1) {
      r.coreAmbiguous.push(n.id);
    } else {
      r.byCoreId.set(n.id, hits[0]);
      used.add(hits[0]);
    }
  });
  osmd.forEach((_, i) => {
    if (!used.has(i)) r.osmdUnmatched.push(i);
  });
  return r;
}

/** core の展開した小節の列と、OSMD のカーソルが通った小節の列を比べる。 */
export function compareWalk(
  core: CoreExpanded[],
  walk: number[],
): { firstMismatch: number | null } {
  const n = Math.max(core.length, walk.length);
  for (let i = 0; i < n; i++) {
    if (core[i]?.measure_index !== walk[i]) return { firstMismatch: i };
  }
  return { firstMismatch: null };
}

/** 2段目の結果。 */
export type SoundingCheck = {
  /** 書かれた音符の組の全部が OSMD の音符に当たり、その回にカーソルが来る鳴る音の数。 */
  resolved: number;
  /** そうでない鳴る音（添字と理由）。 */
  unresolved: { index: number; reason: string }[];
};

/**
 * 鳴る音ごとに、書かれた音符の組のそれぞれが OSMD の音符に当たり、
 * カーソルがその小節を何回目かに通るか（walk の中にその回数以上あるか）を確かめる。
 */
export function checkSounding(
  sounding: Pick<CoreSounding, "written">[],
  byCoreId: Map<string, number>,
  measureOf: Map<string, number>,
  walk: number[],
): SoundingCheck {
  const visits = new Map<number, number>();
  for (const m of walk) visits.set(m, (visits.get(m) ?? 0) + 1);
  const r: SoundingCheck = { resolved: 0, unresolved: [] };
  sounding.forEach((s, index) => {
    for (const w of s.written) {
      if (!byCoreId.has(w.id)) {
        r.unresolved.push({ index, reason: `${w.id} に OSMD の音符がない` });
        return;
      }
      const m = measureOf.get(w.id) ?? -1;
      if ((visits.get(m) ?? 0) < w.pass) {
        r.unresolved.push({
          index,
          reason: `${w.id} の ${w.pass} 回目にカーソルが来ない`,
        });
        return;
      }
    }
    r.resolved += 1;
  });
  return r;
}
