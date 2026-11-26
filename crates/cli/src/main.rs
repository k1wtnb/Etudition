//! 画面なしで core と engine を動かす（`etudition-cli`）。
//!
//! - `etudition-cli s1 <楽譜のパス>`：書かれた音符、展開後の小節、鳴る音、読めない要素を JSON で出す（S1）。
//! - `etudition-cli s1-xml <楽譜のパス>`：楽譜の XML の文字列を出す（`.mxl` は展開する。OSMD に渡す用）。
//! - `etudition-cli make-errors <楽譜のパス> <出力のフォルダ>`：誤りの種類ごとの MIDI を作る（計画 1a-3。種類は `errors::CASES`。
//!   合成の楽譜 T1 だけを受け付ける）。
//! - `etudition-cli click-check <秒> <テンポ（四分音符／分）> <音量 0〜1> <出力.jsonl>`：cpal で 4/4 のクリックを鳴らし、
//!   コールバックごとの時刻の組を1行ずつ書き、ずれと遅れの最小・平均・最大を出す（S2a）。音量 0 なら音を出さない。
//! - `etudition-cli midi-log <秒> <出力.jsonl>`：MIDI の入力（最初の機器）を受け、`EngineClock` の時刻と midir の時刻と
//!   バイト列を1行ずつ書く（F0 と S2b の受信を記録する最小のプログラム）。

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
    if let [cmd, secs, out] = args.as_slice()
        && cmd == "midi-log"
    {
        return midi_log(secs, std::path::Path::new(out));
    }
    if let [cmd, secs, bpm, volume, out] = args.as_slice()
        && cmd == "click-check"
    {
        return click_check(secs, bpm, volume, std::path::Path::new(out));
    }
    let (cmd, path) = match args.as_slice() {
        [cmd, path] if cmd == "s1" || cmd == "s1-xml" => (cmd.as_str(), path),
        _ => {
            eprintln!(
                "etudition-cli {}\n使い方: etudition-cli s1 <楽譜のパス> | etudition-cli s1-xml <楽譜のパス> | etudition-cli make-errors <楽譜のパス> <出力のフォルダ> | etudition-cli click-check <秒> <テンポ> <音量 0〜1> <出力.jsonl> | etudition-cli midi-log <秒> <出力.jsonl>",
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

/// S2a：cpal でクリックを鳴らし、時刻の組を記録する。
fn click_check(secs: &str, bpm: &str, volume: &str, out: &std::path::Path) -> ExitCode {
    use etudition_engine::click::{AudioMsg, AudioOut, CpalOut};
    use etudition_engine::clock::EngineClock;
    use std::fmt::Write as _;
    use std::time::{Duration, Instant};

    let (Ok(secs), Ok(bpm), Ok(volume)) = (
        secs.parse::<u32>(),
        bpm.parse::<u32>(),
        volume.parse::<f32>(),
    ) else {
        eprintln!("秒とテンポは正の整数、音量は 0〜1 の数");
        return ExitCode::from(2);
    };
    if secs == 0 || bpm == 0 {
        eprintln!("秒とテンポは正の整数");
        return ExitCode::from(2);
    }
    let interval_ns = 60_000_000_000 / i64::from(bpm);
    let plan = etudition_cli::click_check::plan(
        etudition_cli::click_check::click_count(secs, bpm),
        interval_ns,
    );
    let (tx, mut rx) = etudition_engine::rtrb::RingBuffer::new(8192);
    let mut audio = CpalOut::new(volume).with_callback_log();
    let lead = Duration::from_millis(300);
    if let Err(e) = audio.start(EngineClock::new(Instant::now() + lead), &plan, tx) {
        eprintln!("鳴らせない: {e}");
        return ExitCode::FAILURE;
    }
    let until =
        Instant::now() + lead + Duration::from_secs(u64::from(secs)) + Duration::from_millis(500);
    let mut stamps = Vec::new();
    let mut lines = String::new();
    let mut lost = false;
    let mut take = |m: AudioMsg| match m {
        AudioMsg::Stamp(s) => {
            let _ = writeln!(lines, "{}", etudition_cli::click_check::stamp_json(&s));
            stamps.push(s);
        }
        AudioMsg::Callback(l) => {
            let _ = writeln!(lines, "{}", etudition_cli::click_check::callback_json(&l));
        }
        AudioMsg::Lost => lost = true,
    };
    while Instant::now() < until {
        while let Ok(m) = rx.pop() {
            take(m);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    audio.stop();
    while let Ok(m) = rx.pop() {
        take(m);
    }
    if let Err(e) = std::fs::write(out, lines) {
        eprintln!("{} を書けない: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let sum = etudition_cli::click_check::summarize(&stamps);
    println!(
        "クリック {} 個（予定 {} 個）。鳴った時刻 − 予定時刻（ms）：最小 {:.3} 平均 {:.3} 最大 {:.3}。再生までの遅れ（ms）：最小 {:.3} 平均 {:.3} 最大 {:.3}。出力先を失った：{}",
        sum.count,
        plan.clicks.len(),
        ms(sum.offset_ns.0),
        sum.offset_ns.1 / 1e6,
        ms(sum.offset_ns.2),
        ms(sum.latency_ns.0),
        sum.latency_ns.1 / 1e6,
        ms(sum.latency_ns.2),
        if lost { "はい" } else { "いいえ" }
    );
    // 欠けたクリックや出力先の喪失があれば、失敗で終える（測り直しが要ることを示す）
    if lost || sum.count < plan.clicks.len() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// F0・S2b：MIDI の入力を `secs` 秒受け、1行ずつ記録する。
fn midi_log(secs: &str, out: &std::path::Path) -> ExitCode {
    use etudition_engine::clock::EngineClock;
    use etudition_engine::input::{InputMsg, InputSink, MidiInput, MidirInput};
    use std::fmt::Write as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    let Ok(secs) = secs.parse::<u64>() else {
        eprintln!("秒は正の整数");
        return ExitCode::from(2);
    };
    let (tx, mut rx) = etudition_engine::rtrb::RingBuffer::new(8192);
    let overflow = Arc::new(AtomicU32::new(0));
    let mut input = MidirInput::new();
    if let Err(e) = input.open(
        EngineClock::new(Instant::now()),
        InputSink {
            producer: tx,
            overflow: Arc::clone(&overflow),
        },
    ) {
        eprintln!("受けられない: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "受ける機器：{}（{secs} 秒）",
        input.port_name.as_deref().unwrap_or("?")
    );
    let until = Instant::now() + Duration::from_secs(secs);
    let mut lines = String::new();
    let (mut count, mut disconnected) = (0usize, false);
    while Instant::now() < until && !disconnected {
        while let Ok(m) = rx.pop() {
            match m {
                InputMsg::Midi(r) => {
                    let _ = writeln!(lines, "{}", etudition_cli::midi_log::raw_json(&r));
                    count += 1;
                }
                InputMsg::Disconnected => disconnected = true,
                InputMsg::Finished => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    input.close();
    if let Err(e) = std::fs::write(out, lines) {
        eprintln!("{} を書けない: {e}", out.display());
        return ExitCode::FAILURE;
    }
    println!(
        "受けたメッセージ {count} 個。あふれ {} 件。切断：{}",
        overflow.load(Ordering::Relaxed),
        if disconnected {
            "あった"
        } else {
            "なかった"
        }
    );
    ExitCode::SUCCESS
}

#[allow(clippy::cast_precision_loss)]
fn ms(ns: i64) -> f64 {
    ns as f64 / 1e6
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
