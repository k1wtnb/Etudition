// S1 のページのデータを作る。合成の楽譜（fixtures/s1/*.musicxml）と、引数で渡した手元の楽譜を、
// etudition-cli で JSON と XML にして app/ui/s1-data/ に置く（版の管理の外。.gitignore）。
// 使い方：pnpm -C app/ui run s1-data [手元の楽譜のパス ...]
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ui = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(ui, "../..");
const out = join(ui, "s1-data");
const fixtures = join(repo, "fixtures", "s1");

const scores = [
  ...readdirSync(fixtures)
    .filter((f) => f.endsWith(".musicxml"))
    .sort()
    .map((f) => join(fixtures, f)),
  ...process.argv.slice(2).map((p) => resolve(p)),
];

function cli(cmd, path) {
  return execFileSync(
    "cargo",
    ["run", "-q", "-p", "etudition-cli", "--", cmd, path],
    { cwd: repo, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const index = scores.map((path, i) => {
  const name = basename(path, extname(path));
  const id = `${String(i).padStart(2, "0")}`;
  writeFileSync(join(out, `${id}.json`), cli("s1", path));
  writeFileSync(join(out, `${id}.xml`), cli("s1-xml", path));
  console.log(`${id}: ${path}`);
  return { name, json: `${id}.json`, xml: `${id}.xml` };
});
writeFileSync(join(out, "index.json"), JSON.stringify(index, null, 2));
console.log(`${index.length} 件を ${out} に書いた`);
