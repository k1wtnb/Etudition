// S1 のページ（アプリ本体の画面とは別の入口）。cli の JSON と OSMD の音符を突き合わせ、結果を window.s1Result と表に出す。
// データは app/ui/s1-data/（版の管理の外）に `pnpm run s1-data` で作る。
import { type Note, OpenSheetMusicDisplay } from "opensheetmusicdisplay";
import { checkSounding, compareWalk, matchWritten, type S1Json } from "./match";
import {
  drawMark,
  extractNotes,
  moveCursorTo,
  resetMarkLayer,
  type WalkStep,
  walkCursor,
} from "./osmd";

type IndexEntry = { name: string; json: string; xml: string };

/** 1つの楽譜の突き合わせの結果（取りまとめ役がブラウザで読む）。 */
type S1Result = {
  score: string;
  error?: string;
  writtenTotal?: number;
  writtenMatched?: number;
  coreMissing?: string[];
  coreAmbiguous?: string[];
  osmdTotal?: number;
  osmdUnmatched?: string[];
  walkCore?: number[];
  walkOsmd?: number[];
  walkFirstMismatch?: number | null;
  soundingTotal?: number;
  soundingResolved?: number;
  soundingUnresolved?: { index: number; reason: string }[];
  coreIssues?: S1Json["issues"];
  stage1Pass?: boolean;
  stage2Pass?: boolean;
};

/** 鳴る音を選んだ結果。 */
type SelectResult = {
  index: number;
  marks: number;
  cursorStep: number;
  cursorMeasureIndex?: number;
  cursorVisit?: number;
  error?: string;
};

declare global {
  interface Window {
    s1Result?: S1Result;
    s1Select?: (index: number) => SelectResult;
  }
}

const DATA = "/s1-data";

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (text !== undefined) e.textContent = text;
  return e;
}

function fmt(f: { num: number; den: number }): string {
  return f.den === 1 ? String(f.num) : `${f.num}/${f.den}`;
}

async function fetchText(path: string): Promise<string> {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path} を読めない（${r.status}）`);
  return r.text();
}

async function loadScore(entry: IndexEntry, root: HTMLElement): Promise<void> {
  root.replaceChildren();
  window.s1Result = { score: entry.name };
  const view = el("div");
  view.id = "s1-osmd";
  const summary = el("div");
  const table = el("table");
  root.append(view, summary, table);
  try {
    const json = JSON.parse(await fetchText(`${DATA}/${entry.json}`)) as S1Json;
    const xml = await fetchText(`${DATA}/${entry.xml}`);
    const osmd = new OpenSheetMusicDisplay(view, {
      backend: "svg",
      autoResize: false,
      drawTitle: false,
      renderSingleHorizontalStaffline: true,
    });
    await osmd.load(xml);
    osmd.render();

    const { notes, info } = extractNotes(osmd);
    const match = matchWritten(json.written, info);
    const walk = walkCursor(osmd);
    const walkCore = json.expanded_measures.map((m) => m.measure_index);
    const measureOf = new Map(json.written.map((w) => [w.id, w.measure_index]));
    const sounding = checkSounding(
      json.sounding,
      match.byCoreId,
      measureOf,
      walk.measures,
    );
    const walkMismatch = compareWalk(
      json.expanded_measures,
      walk.measures,
    ).firstMismatch;
    const result: S1Result = {
      score: entry.name,
      writtenTotal: json.written.length,
      writtenMatched: match.byCoreId.size,
      coreMissing: match.coreMissing,
      coreAmbiguous: match.coreAmbiguous,
      osmdTotal: info.length,
      osmdUnmatched: match.osmdUnmatched.map((i) => JSON.stringify(info[i])),
      walkCore,
      walkOsmd: walk.measures,
      walkFirstMismatch: walkMismatch,
      soundingTotal: json.sounding.length,
      soundingResolved: sounding.resolved,
      soundingUnresolved: sounding.unresolved,
      coreIssues: json.issues,
      stage1Pass:
        match.byCoreId.size === json.written.length &&
        match.osmdUnmatched.length === 0,
      stage2Pass:
        walkMismatch === null && sounding.resolved === json.sounding.length,
    };
    window.s1Result = result;
    window.s1Select = (index) =>
      selectSounding(
        osmd,
        view,
        json,
        notes,
        match.byCoreId,
        walk.steps,
        index,
      );
    renderSummary(summary, result);
    renderSounding(table, json, sounding.unresolved, (i) =>
      window.s1Select?.(i),
    );
  } catch (e) {
    const message = e instanceof Error ? `${e.message}\n${e.stack}` : String(e);
    window.s1Result = { score: entry.name, error: message };
    summary.append(el("pre", `誤り：${message}`));
  }
}

/** 鳴る音を選び、書かれた音符の組に印を重ね、カーソルをその回の位置へ動かす。 */
function selectSounding(
  osmd: OpenSheetMusicDisplay,
  view: HTMLElement,
  json: S1Json,
  notes: Note[],
  byCoreId: Map<string, number>,
  steps: WalkStep[],
  index: number,
): SelectResult {
  const s = json.sounding[index];
  const r: SelectResult = { index, marks: 0, cursorStep: -1 };
  if (!s) return { ...r, error: "その番号の鳴る音がない" };
  const layer = resetMarkLayer(view);
  if (!layer) return { ...r, error: "OSMD の svg がない" };
  const first = s.written[0];
  const colors = ["#d33", "#1a7", "#36c"];
  for (const w of s.written) {
    const i = byCoreId.get(w.id);
    if (i === undefined) continue;
    const color = colors[(w.pass - 1) % colors.length];
    if (drawMark(osmd, layer, notes[i], `${w.pass}回目`, color)) r.marks += 1;
  }
  const target = notes[byCoreId.get(first.id) ?? -1];
  const measure = json.written.find((w) => w.id === first.id)?.measure_index;
  // カーソルの下にその音符がある歩。なければ（装飾音など）その小節のその回の最初の歩
  let step = steps.findIndex(
    (st) =>
      st.measureIndex === measure &&
      st.visit === first.pass &&
      target !== undefined &&
      st.notes.has(target),
  );
  if (step < 0) {
    step = steps.findIndex(
      (st) => st.measureIndex === measure && st.visit === first.pass,
    );
  }
  r.cursorStep = step;
  if (step >= 0) {
    moveCursorTo(osmd, step);
    r.cursorMeasureIndex = steps[step].measureIndex;
    r.cursorVisit = steps[step].visit;
  } else {
    r.error = "カーソルの歩みに、その回の位置がない";
  }
  return r;
}

function renderSummary(root: HTMLElement, r: S1Result): void {
  const rows: [string, string][] = [
    [
      "1段目（書かれた音符 ↔ OSMD の音符がちょうど1つ）",
      `${r.stage1Pass ? "合格" : "不合格"}：${r.writtenMatched} / ${r.writtenTotal}（OSMD の音符 ${r.osmdTotal}、相手なし ${r.coreMissing?.length}、1つに決まらない ${r.coreAmbiguous?.length}、OSMD の余り ${r.osmdUnmatched?.length}）`,
    ],
    [
      "2段目（鳴る音 → 書かれた音符の組と何回目か → OSMD の位置）",
      `${r.stage2Pass ? "合格" : "不合格"}：${r.soundingResolved} / ${r.soundingTotal}、カーソルの歩みと展開の最初の食い違い：${r.walkFirstMismatch ?? "なし"}`,
    ],
    ["展開（core の小節の列）", (r.walkCore ?? []).join(" ")],
    ["カーソル（OSMD の小節の列）", (r.walkOsmd ?? []).join(" ")],
    [
      "読めない要素（core）",
      (r.coreIssues ?? [])
        .map((i) => `${i.element}（小節 ${i.measure_index}）`)
        .join("、") || "なし",
    ],
  ];
  const t = el("table");
  for (const [k, v] of rows) {
    const tr = el("tr");
    tr.append(el("th", k), el("td", v));
    t.append(tr);
  }
  root.append(t);
}

function renderSounding(
  table: HTMLTableElement,
  json: S1Json,
  unresolved: { index: number; reason: string }[],
  onSelect: (index: number) => void,
): void {
  const head = el("tr");
  for (const h of ["#", "音高", "位置", "長さ", "書かれた音符#何回目", "2段目"])
    head.append(el("th", h));
  table.append(head);
  const why = new Map(unresolved.map((u) => [u.index, u.reason]));
  json.sounding.forEach((s, i) => {
    const tr = el("tr");
    tr.tabIndex = 0;
    tr.append(
      el("td", String(i)),
      el("td", `${s.pitch}${s.is_grace ? "（装飾音）" : ""}`),
      el("td", fmt(s.start_quarters)),
      el("td", fmt(s.duration_quarters)),
      el("td", s.written.map((w) => `${w.id}#${w.pass}`).join(" ")),
      el("td", why.get(i) ?? "ok"),
    );
    tr.addEventListener("click", () => onSelect(i));
    table.append(tr);
  });
}

async function main(): Promise<void> {
  const app = document.getElementById("s1");
  if (!app) return;
  const select = el("select");
  const root = el("div");
  app.append(select, root);
  let entries: IndexEntry[] = [];
  try {
    entries = JSON.parse(await fetchText(`${DATA}/index.json`));
  } catch (e) {
    window.s1Result = { score: "", error: String(e) };
    app.append(
      el(
        "pre",
        `データがない。先に pnpm -C app/ui run s1-data を実行する。${e}`,
      ),
    );
    return;
  }
  entries.forEach((e, i) => {
    const o = el("option", e.name);
    o.value = String(i);
    select.append(o);
  });
  const fromUrl = new URLSearchParams(location.search).get("score");
  const start = Math.max(
    0,
    entries.findIndex((e) => e.name === fromUrl),
  );
  select.value = String(start);
  select.addEventListener("change", () =>
    loadScore(entries[Number(select.value)], root),
  );
  await loadScore(entries[start], root);
}

main();
