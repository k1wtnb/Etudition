<script lang="ts">
// 練習の画面（1a-7）。楽譜を読み、回を始め、通知（LiveEvent）を受けて描く。
// カーソルと印は Sheet が SVG を直接書き換える（Svelte の状態を通さない）。時刻は表示だけに使う（CLAUDE.md の7章）。
import { onDestroy, onMount } from "svelte";
import {
  feedMidiFile,
  type LiveEvent,
  loadScore,
  startPractice,
  stopPractice,
  type TempoView,
  type UiTime,
} from "./lib/commands";
import { type Outcome, outcomeOf } from "./lib/outcome";
import { markKind, markStyle, noteName, quartersAt } from "./lib/present";
import { Sheet, type SheetSummary } from "./lib/sheet";

// 流し込みの操作は開発用だけに出す（REQ-106。通常のビルドにはコマンドもない。AC-23）
const devFeed = import.meta.env.DEV;

let scorePath = $state("");
let midiPath = $state("");
let bpm = $state(60);
let summary: SheetSummary | null = $state(null);
let running = $state(false);
let message = $state("");
let received: { note: number; count: number } | null = $state(null);
let outcome: Outcome | null = $state(null);

let container: HTMLDivElement;
let sheet: Sheet | null = null;
let frame = 0;
// 回の始まり：前カウントの最初のクリック（時刻 0）を、Started を受けた時刻とみなす（表示だけ）
let clock: { startedAt: number; origin: UiTime; tempo: TempoView } | null =
  null;

onMount(() => {
  sheet = new Sheet(container);
});
onDestroy(() => cancelAnimationFrame(frame));

async function load() {
  message = "";
  outcome = null;
  summary = null;
  try {
    const score = await loadScore(scorePath.trim());
    summary = (await sheet?.load(score)) ?? null;
  } catch (e) {
    message = String(e);
  }
}

function tick() {
  if (!clock || !sheet) return;
  const now: UiTime = { ns: (performance.now() - clock.startedAt) * 1e6 };
  sheet.showCursorAt(quartersAt(now, clock.origin, clock.tempo));
  frame = requestAnimationFrame(tick);
}

function onEvent(ev: LiveEvent) {
  switch (ev.type) {
    case "started":
      clock = {
        startedAt: performance.now(),
        origin: ev.origin,
        tempo: ev.tempo,
      };
      frame = requestAnimationFrame(tick);
      break;
    case "click":
      // クリックは 1a-5 で来る。カーソルは origin と tempo から求めた拍の位置で進める（B0 の5章）
      break;
    case "note_received":
      received = { note: ev.note, count: (received?.count ?? 0) + 1 };
      break;
    case "marks":
      for (const m of ev.marks)
        sheet?.mark(m.ref_index, markStyle(markKind(m)));
      break;
    case "ended":
    case "invalid":
      finish(ev);
      break;
  }
}

function finish(ev: LiveEvent) {
  cancelAnimationFrame(frame);
  clock = null;
  running = false;
  outcome = outcomeOf(ev);
  if (!outcome || !sheet) return;
  // 演奏中の仮の印は消す。止まった回（W03）と正常に終わった回（Q-34 の B）は、演奏後の判定で間違えた音を描き直す。
  // 無効の回（Q-52 の C）は採点しないので、印を残さない。
  sheet.clearMarks();
  if (outcome.kind === "invalid") return;
  for (const m of outcome.mistakes)
    sheet.mark(m.refIndex, markStyle(m.kind), m.detail);
  if (outcome.kind === "stopped" && outcome.stoppedAt !== null) {
    sheet.mark(
      outcome.stoppedAt,
      { shape: "dash", color: "#0969da", label: "止" },
      "ここで止まった",
      false,
    );
  }
}

function begin(): boolean {
  if (!summary) {
    message = "先に楽譜を読んでください";
    return false;
  }
  message = "";
  outcome = null;
  received = null;
  sheet?.clearMarks();
  running = true;
  return true;
}

async function start() {
  if (!begin()) return;
  try {
    await startPractice({ num: bpm, den: 1 }, onEvent);
  } catch (e) {
    running = false;
    message = `始められない：${e}`;
  }
}

async function feed() {
  if (!begin()) return;
  try {
    await feedMidiFile(midiPath.trim(), { num: bpm, den: 1 }, onEvent);
  } catch (e) {
    running = false;
    message = `始められない：${e}`;
  }
}

async function stop() {
  try {
    await stopPractice();
  } catch (e) {
    message = String(e);
  }
  cancelAnimationFrame(frame);
  clock = null;
  running = false;
}
</script>

<main>
  <section class="controls">
    <label>楽譜（MusicXML のパス）<input bind:value={scorePath} size="50" /></label>
    <button type="button" onclick={load} disabled={running}>読む</button>
    <label>テンポ（四分音符／分）<input type="number" min="1" bind:value={bpm} /></label>
    <button type="button" onclick={start} disabled={running}>始める</button>
    <button type="button" onclick={stop} disabled={!running}>止める</button>
  </section>
  {#if devFeed}
    <section class="controls dev">
      <span>開発用：</span>
      <label>MIDI ファイルのパス<input bind:value={midiPath} size="50" /></label>
      <button type="button" onclick={feed} disabled={running}>流し込む</button>
    </section>
  {/if}

  {#if message}<p class="message" role="alert">{message}</p>{/if}
  {#if summary}
    <p class="summary">
      書かれた音符 {summary.writtenMatched}/{summary.written}、お手本の音 {summary.refsResolved}/{summary.refs}
      が楽譜の上の位置に対応した
    </p>
  {/if}
  {#if received}
    <p class="received">受けた音：{noteName(received.note)}（{received.note}）　{received.count} 音目</p>
  {/if}

  <div class="sheet" bind:this={container}></div>

  {#if outcome}
    <section class="outcome">
      {#if outcome.kind === "scored"}
        <p class="verdict">{outcome.passed ? "合格" : "不合格"}{outcome.cap ? "（終わりの上限で終えた）" : ""}</p>
        <table>
          <thead><tr><th>項目</th><th>点数</th><th>誤り</th></tr></thead>
          <tbody>
            {#each outcome.items as it}
              <tr><td>{it.name}</td><td>{it.score}</td><td>{it.errors}/{it.of}</td></tr>
            {/each}
          </tbody>
        </table>
        <p>
          {outcome.mistakes.length > 0
            ? `演奏後の判定で間違えた音（${outcome.mistakes.length} 音）を楽譜に示しました。`
            : "演奏後の判定で間違えた音はありません。"}
        </p>
      {:else if outcome.kind === "stopped"}
        <p class="verdict">止まったので、点数は出しません。止まった位置までの間違えた場所を楽譜に示しました。</p>
        {#if outcome.extras > 0}
          <p>楽譜にない音を {outcome.extras} 回弾きました（楽譜の上には示していません）。</p>
        {/if}
      {:else}
        <p class="verdict">この回は無効になりました（点数は出しません）：{outcome.reason}</p>
        <p>{outcome.advice}</p>
      {/if}
      {#if outcome.recordError !== null}
        <p class="message" role="alert">記録できなかった：{outcome.recordError}</p>
      {/if}
    </section>
  {/if}
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 8px 16px;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
  }
  .dev {
    border: 1px dashed #9a6700;
    padding: 4px 8px;
  }
  .sheet {
    overflow-x: auto;
    background: #fff;
  }
  .message {
    color: #cf222e;
  }
  .verdict {
    font-size: 1.3em;
    font-weight: bold;
  }
  table {
    border-collapse: collapse;
  }
  td,
  th {
    border: 1px solid #d0d7de;
    padding: 2px 10px;
    text-align: right;
  }
</style>
