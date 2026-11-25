<script lang="ts">
import { onMount } from "svelte";
import { p0TimeProbe } from "./lib/commands";

// P0 の④の確かめ：Rust から受け取った時刻の値と型を表示する
let probe = $state("確かめ中");

onMount(async () => {
  try {
    const t = await p0TimeProbe();
    const exact = t.ns === Number.MAX_SAFE_INTEGER;
    probe = `ns=${t.ns}（${typeof t.ns}、上限と一致：${exact}）`;
  } catch (e) {
    probe = `失敗：${String(e)}`;
  }
});
</script>

<main>
  <h1>Etudition</h1>
  <p>P0 の空のアプリ</p>
  <p>時刻の確かめ：{probe}</p>
</main>
