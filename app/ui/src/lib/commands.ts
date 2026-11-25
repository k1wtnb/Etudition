// Tauri のコマンドを呼ぶ薄い関数。名前と引数は Rust の側（app/src-tauri）と合わせる（計画 2-3 の3）。
import { invoke } from "@tauri-apps/api/core";

/** 画面に渡す時刻（前カウントの最初のクリックを 0 とする、符号付きのナノ秒。Rust の UiTime） */
export type UiTime = { ns: number };

/** P0 の④の確かめ：Rust の `p0_time_probe` を呼ぶ */
export function p0TimeProbe(): Promise<UiTime> {
  return invoke<UiTime>("p0_time_probe");
}
