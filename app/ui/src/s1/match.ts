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

/** カーソルの1歩（OSMD に触らない形）。何回目にその小節を通っているかと、カーソルの下の音符（OSMD の音符の添字）。 */
export type CursorStep = {
  measureIndex: number;
  visit: number;
  notes: Set<number>;
};

/** 2段目の結果。 */
export type SoundingCheck = {
  /** 書かれた音符の組の全部が、OSMD の音符に当たり、その音を含むその回の歩に解決し、印を描ける鳴る音の数。 */
  resolved: number;
  /** そうでない鳴る音（添字と、最初に見つけた理由）。 */
  unresolved: { index: number; reason: string }[];
  /** 鳴る音ごとの、カーソルを置く歩（組の最初の音符。代わりの位置を含む。なければ -1）。 */
  stepOf: number[];
  /** その音を含む歩がなく、その回の小節の最初の歩（代わりの位置）になった書かれた音符の数。 */
  fallbackRefs: number;
  /** 印を描けない書かれた音符の数。 */
  markFailures: number;
};

/**
 * 鳴る音ごとに、書かれた音符の組のそれぞれが、OSMD の音符に当たり、
 * カーソルの歩みの中の「その小節の、その回の、その音を含む歩」に解決するか、印を描けるかを確かめる。
 * その音を含む歩がなく、その回の小節の最初の歩で代わりにした場合は、不合格に数える。
 */
export function checkSounding(
  sounding: Pick<CoreSounding, "written">[],
  byCoreId: Map<string, number>,
  measureOf: Map<string, number>,
  steps: CursorStep[],
  canMark: (osmdIndex: number) => boolean,
): SoundingCheck {
  const r: SoundingCheck = {
    resolved: 0,
    unresolved: [],
    stepOf: [],
    fallbackRefs: 0,
    markFailures: 0,
  };
  sounding.forEach((s, index) => {
    let reason: string | undefined;
    let firstStep: number | undefined;
    for (const w of s.written) {
      const o = byCoreId.get(w.id);
      let step = -1;
      if (o === undefined) {
        reason ??= `${w.id} に OSMD の音符がない`;
      } else {
        const m = measureOf.get(w.id) ?? -1;
        const inVisit = (st: CursorStep) =>
          st.measureIndex === m && st.visit === w.pass;
        step = steps.findIndex((st) => inVisit(st) && st.notes.has(o));
        if (step < 0) {
          step = steps.findIndex(inVisit);
          if (step < 0) {
            reason ??= `${w.id} の ${w.pass} 回目にカーソルが来ない`;
          } else {
            r.fallbackRefs += 1;
            reason ??= `${w.id} の ${w.pass} 回目は、その音を含む歩がなく代わりの位置（小節の頭）になる`;
          }
        }
        if (!canMark(o)) {
          r.markFailures += 1;
          reason ??= `${w.id} の印を描けない`;
        }
      }
      firstStep ??= step;
    }
    r.stepOf.push(firstStep ?? -1);
    if (reason === undefined) r.resolved += 1;
    else r.unresolved.push({ index, reason });
  });
  return r;
}
