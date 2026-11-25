// npm 側の依存のライセンスを、許可の一覧（Etudition-dev の requirements.md の REQ-099）と照らす。
// 許可の一覧は、Rust 側の deny.toml と同じ。許可にないライセンスがあれば、終了コード 1 で止まる。
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const ALLOWED = new Set([
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unlicense",
  "CC0-1.0",
  "BlueOak-1.0.0",
  "0BSD",
]);

// MPL-2.0 の部品ごとの例外。改変せずに使う（許可表 REQ-099「MPL-2.0 は改変せずに使う場合だけ使ってよい」）。
// lightningcss は Vite が使う CSS の処理系（2026-11-25 に P0 で足した）。名前の先頭で照らす（OS ごとの部品を含めるため）
const MPL_EXCEPTIONS = ["lightningcss"];

// AND を含む式の、部品ごとの確かめ済みの例外（部品の名前と、式そのものが両方一致したときだけ通す）。
// pako（OSMD の jszip が使う）：「(MIT AND Zlib)」。どちらも許可の一覧にある。原文の LICENSE を確かめた（2026-11-25、S1）
const AND_EXCEPTIONS = new Map([["pako", "(MIT AND Zlib)"]]);

/**
 * 1つのライセンス、または「A OR B」（全体を括弧で囲んでもよい）だけを判定し、どれか1つが許可されていれば通す。
 * AND や入れ子の括弧を含む式は解釈せず、通さない（初めて出てきたときに人が確かめる）。
 */
export function isAllowed(expr, name) {
  if (
    expr === "MPL-2.0" &&
    MPL_EXCEPTIONS.some(
      (prefix) => name === prefix || name.startsWith(`${prefix}-`),
    )
  ) {
    return true;
  }
  if (AND_EXCEPTIONS.get(name) === expr) {
    return true;
  }
  const inner = /^\((.*)\)$/.test(expr) ? expr.slice(1, -1) : expr;
  if (/[()]/.test(inner) || /\sAND\s/.test(inner)) {
    return false;
  }
  return inner.split(/\s+OR\s+/).some((one) => ALLOWED.has(one.trim()));
}

function main() {
  const out = execFileSync("pnpm", ["licenses", "list", "--json"], {
    encoding: "utf8",
  });
  const byLicense = JSON.parse(out);
  const bad = [];
  for (const [license, packages] of Object.entries(byLicense)) {
    for (const p of packages) {
      if (!isAllowed(license, p.name)) {
        bad.push(
          `${p.name}@${p.versions ? p.versions.join(",") : p.version} : ${license}`,
        );
      }
    }
  }
  const count = Object.values(byLicense).reduce(
    (n, list) => n + list.length,
    0,
  );
  if (bad.length > 0) {
    console.error(`許可にないライセンスの依存が ${bad.length} 件ある：`);
    for (const line of bad) console.error(`  ${line}`);
    process.exit(1);
  }
  console.log(`ok: ${count} 件の依存のライセンスは、すべて許可の一覧にある`);
}

// 直接実行したときだけ動く（試験から isAllowed を読み込めるようにする）
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main();
}
