// Tauri のコマンドを呼ぶ薄い関数。名前と引数は Rust の側（app/src-tauri）と合わせる（計画 2-3 の3）。
// 型は Rust から ts-rs で作ったもの（./bindings。手で書き換えない）。
import { Channel, invoke } from "@tauri-apps/api/core";
import type { LiveEvent } from "./bindings/LiveEvent";
import type { ScoreView } from "./bindings/ScoreView";
import type { TempoView } from "./bindings/TempoView";
import type { UiTime } from "./bindings/UiTime";

export type { LiveEvent, ScoreView, TempoView, UiTime };

/** P0 の④の確かめ：Rust の `p0_time_probe` を呼ぶ（1a-6 の本物のコマンドで時刻の受け渡しを確かめたら消す。BQ-03） */
export function p0TimeProbe(): Promise<UiTime> {
  return invoke<UiTime>("p0_time_probe");
}

/** 楽譜を読む（Rust の `load_score`）。採点に使えない楽譜は、理由の文字列で失敗する */
export function loadScore(path: string): Promise<ScoreView> {
  return invoke<ScoreView>("load_score", { path });
}

/** 通知を受ける Channel を作る */
function channelTo(onEvent: (ev: LiveEvent) => void): Channel<LiveEvent> {
  const events = new Channel<LiveEvent>();
  events.onmessage = onEvent;
  return events;
}

/** 練習を始める（Rust の `start_practice`）。実機の入力は 1a-8 で足す */
export function startPractice(
  tempo: TempoView,
  onEvent: (ev: LiveEvent) => void,
): Promise<void> {
  return invoke<void>("start_practice", { tempo, events: channelTo(onEvent) });
}

/** 練習を終える（Rust の `stop_practice`。使う人が閉じた回は記録しない） */
export function stopPractice(): Promise<void> {
  return invoke<void>("stop_practice");
}

/** 開発用のビルドだけ：MIDI ファイルを流し込む（Rust の `feed_midi_file`。REQ-106） */
export function feedMidiFile(
  path: string,
  tempo: TempoView,
  onEvent: (ev: LiveEvent) => void,
): Promise<void> {
  return invoke<void>("feed_midi_file", {
    path,
    tempo,
    events: channelTo(onEvent),
  });
}
