//! 画面なしで core と engine を動かす（`etudition-cli`）。
//!
//! - `etudition-cli s1 <楽譜のパス>`：書かれた音符、展開後の小節、鳴る音、読めない要素を JSON で出す（S1）。
//! - `etudition-cli s1-xml <楽譜のパス>`：楽譜の XML の文字列を出す（`.mxl` は展開する。OSMD に渡す用）。
//! - `etudition-cli make-errors <楽譜のパス> <出力のフォルダ>`：誤りの種類ごとの MIDI を作る（計画 1a-3。種類は `errors::CASES`。
//!   合成の楽譜 T1 だけを受け付ける）。

use std::process::ExitCode;

use etudition_core::expand::expand;
use etudition_core::quarters::Quarters;
use etudition_core::score::{LoadedScore, ReadError, read_score};
use serde_json::{Value, json};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [cmd, path, out] = args.as_slice()
        && cmd == "make-errors"
    {
        return make_errors(path, std::path::Path::new(out));
    }
    let (cmd, path) = match args.as_slice() {
        [cmd, path] if cmd == "s1" || cmd == "s1-xml" => (cmd.as_str(), path),
        _ => {
            eprintln!(
                "etudition-cli {}\n使い方: etudition-cli s1 <楽譜のパス> | etudition-cli s1-xml <楽譜のパス> | etudition-cli make-errors <楽譜のパス> <出力のフォルダ>",
                env!("CARGO_PKG_VERSION")
            );
            return ExitCode::from(2);
        }
    };
    let loaded = std::fs::read(path)
        .map_err(|e| ReadError(format!("{path} を開けない: {e}")))
        .and_then(|bytes| read_score(&bytes));
    match loaded {
        Ok(loaded) if cmd == "s1" => {
            println!("{:#}", s1_json(&loaded));
            ExitCode::SUCCESS
        }
        Ok(loaded) => {
            print!("{}", loaded.xml);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("読めない: {e}");
            ExitCode::FAILURE
        }
    }
}

/// 誤りの種類ごとの MIDI を `out` に書く（`<名前>.mid`）。
fn make_errors(path: &str, out: &std::path::Path) -> ExitCode {
    // Windows で取り出した写しは改行が CRLF のことがあるので、LF にそろえて比べる
    let normalized = |b: &[u8]| String::from_utf8_lossy(b).replace('\r', "");
    let bytes = match std::fs::read(path) {
        Ok(b) if normalized(&b) == normalized(etudition_cli::errors::T1) => b,
        Ok(_) => {
            eprintln!("make-errors は合成の楽譜 T1（fixtures/1a/t1.musicxml）だけを受け付ける");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("{path} を開けない: {e}");
            return ExitCode::FAILURE;
        }
    };
    // 照らしたバイト列をそのまま読む（読み直さない）
    let loaded = match read_score(&bytes) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("読めない: {e}");
            return ExitCode::FAILURE;
        }
    };
    let e = expand(&loaded.score);
    if let Err(issues) = etudition_core::expand::check_playable(&e) {
        eprintln!("採点に使えない楽譜: {issues:?}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = std::fs::create_dir_all(out) {
        eprintln!("出力のフォルダを作れない: {err}");
        return ExitCode::FAILURE;
    }
    for c in etudition_cli::errors::CASES {
        let midi = etudition_cli::errors::make_error_midi(&e, c.errors);
        if let Err(err) = std::fs::write(out.join(format!("{}.mid", c.name)), midi) {
            eprintln!("{} を書けない: {err}", c.name);
            return ExitCode::FAILURE;
        }
        println!(
            "{}.mid（{}、期待：音程 {} タイミング {} 長さ {} 組 {}）",
            c.name, c.row, c.expected.0, c.expected.1, c.expected.2, c.expected.3
        );
    }
    ExitCode::SUCCESS
}

/// 四分音符の数を `{"num": 分子, "den": 分母}` にする。
fn q(x: Quarters) -> Value {
    json!({ "num": x.num(), "den": x.den() })
}

/// S1 の JSON を作る。位置と長さの単位は四分音符の数（名前の末尾 `_quarters`）。原点は1小節目の頭。
fn s1_json(loaded: &LoadedScore) -> Value {
    let score = &loaded.score;
    let e = expand(score);
    // 展開の結果の issues は、読み込みの問題も含む（expand が写す）
    let issues: Vec<Value> = e
        .issues
        .iter()
        .map(|i| json!({ "measure_index": i.measure_index, "element": i.element, "detail": i.detail }))
        .collect();
    json!({
        "part": score.part,
        "measures": score.measures.iter().map(|m| json!({
            "index": m.index,
            "number": m.number,
            "length_quarters": q(m.length),
            "implicit": m.implicit,
        })).collect::<Vec<_>>(),
        "written": score.notes.iter().map(|n| json!({
            "id": n.id.to_string(),
            "part": n.id.part,
            "measure_index": n.id.measure_index,
            "measure_number": n.measure_number,
            "voice": n.id.voice,
            "order": n.id.order,
            "staff": n.staff,
            "pitch": n.pitch,
            "offset_quarters": q(n.offset),
            "duration_quarters": q(n.duration),
            "tie_start": n.tie_start,
            "tie_stop": n.tie_stop,
            "is_grace": n.is_grace,
        })).collect::<Vec<_>>(),
        "expanded_measures": e.measures.iter().map(|m| json!({
            "measure_index": m.measure_index,
            "pass": m.pass,
            "start_quarters": q(m.start),
        })).collect::<Vec<_>>(),
        "sounding": e.sounding.iter().map(|s| json!({
            "pitch": s.pitch,
            "start_quarters": q(s.start),
            "duration_quarters": q(s.duration),
            "is_grace": s.is_grace,
            "written": s.written.iter().map(|w| json!({ "id": w.id.to_string(), "pass": w.pass })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "issues": issues,
    })
}

#[cfg(test)]
mod tests {
    use super::{read_score, s1_json};

    #[test]
    fn s1_json_has_written_sounding_and_issues() {
        let loaded = read_score(include_bytes!(
            "../../../fixtures/s1/05-repeat-volta.musicxml"
        ))
        .expect("読めるはず");
        let v = s1_json(&loaded);
        assert_eq!(v["written"].as_array().map(Vec::len), Some(5));
        assert_eq!(v["written"][0]["id"], "P1/m0/v1/n0");
        assert_eq!(v["sounding"].as_array().map(Vec::len), Some(7));
        assert_eq!(v["sounding"][3]["written"][0]["id"], "P1/m0/v1/n0");
        assert_eq!(v["sounding"][3]["written"][0]["pass"], 2);
        assert_eq!(v["sounding"][3]["start_quarters"]["num"], 12);
        assert_eq!(v["issues"].as_array().map(Vec::len), Some(0));
    }
}
