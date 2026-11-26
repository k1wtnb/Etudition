//! 1a-3：誤り入りの MIDI と回帰試験（AC-06、AC-07。8-4 の 1a の終わりの条件 ① と ②）。
//!
//! 誤り入りの MIDI を `engine` の流し込みで採点し、誤りの数が表（`errors::CASES`）のとおりかを確かめる。
//! 同じ MIDI を2回流して一致すること、`result.json` の q と合否が採点の関数の値と一致すること、
//! 点数の控え（insta）と一致することも確かめる。控えの更新は、差分を人間が見てから（計画 1a-3）。

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use etudition_cli::errors::{CASES, ErrorCase, make_error_midi};
use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::meter::Meter;
use etudition_core::rules::RULES_V0;
use etudition_core::score::read_score;
use etudition_core::scoring::{RunResult, TimingResult};
use etudition_core::timing::{Tempo, Timing};
use etudition_engine::input::FileInput;
use etudition_engine::record::{RecordStatus, score_hash};
use etudition_engine::session::{
    EngineEvent, PracticeSetup, RunMode, SessionOutcome, open_session,
};

use etudition_cli::errors::T1;
const S: i64 = 1_000_000_000;

fn expansion() -> Expansion {
    let e = expand(&read_score(T1).expect("読めるはず").score);
    check_playable(&e).expect("使えるはず");
    e
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "etudition-regression-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// 誤り入りの MIDI を流して採点する。採点の結果と、`result.json` の中身（書いたとき）を返す。
fn run_case(c: &ErrorCase, dir: &std::path::Path) -> (RunResult, Option<serde_json::Value>) {
    let e = expansion();
    let timing = Timing {
        tempo: Tempo::new(60, 1).unwrap(),
        origin_ns: 4 * S,
    };
    let midi = make_error_midi(&e, c.errors);
    let input = FileInput::new(&midi, &e, &timing).expect("MIDI を読めるはず");
    let setup = PracticeSetup {
        expansion: e,
        score_hash: score_hash(T1),
        timing,
        meter: Meter {
            beats: 4,
            beat_type: 4,
        },
        correction_ns: 0,
        record_dir: dir.to_path_buf(),
        rules: RULES_V0,
    };
    let open = open_session(setup, Box::new(input)).expect("開けるはず");
    let last = std::sync::Mutex::new(None);
    let outcome = open.run(
        RunMode::Feed,
        &|ev| *last.lock().unwrap() = Some(ev),
        &AtomicBool::new(false),
    );
    let SessionOutcome::Scored(result) = outcome else {
        panic!("{}：採点されるはず: {outcome:?}", c.name);
    };
    let json = match last.into_inner().unwrap() {
        Some(EngineEvent::Ended {
            record: RecordStatus::Written(files),
            ..
        }) => files
            .json
            .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()),
        _ => None,
    };
    (result, json)
}

/// AC-06：誤りの種類ごとに、意図した項目の誤りの数が表のとおりで、ほかの項目は変わらない。
#[test]
fn error_cases_match_the_table() {
    let base = tmp("table");
    for c in CASES {
        let (r, _) = run_case(c, &base.join(c.name));
        let i = r.items.expect("点数があるはず");
        let pairs = r.notes.iter().filter(|n| n.played.is_some()).count();
        assert_eq!(
            (i.pitch.errors, i.timing.errors, i.duration.errors, pairs),
            c.expected,
            "{}（{}）",
            c.name,
            c.row
        );
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// AC-07：同じ MIDI を2回流すと、採点の関数の出力が一致する。`result.json` の q と合否も一致する。
#[test]
fn same_input_same_output_and_saved_scores_match() {
    let base = tmp("twice");
    for c in CASES {
        let (a, json) = run_case(c, &base.join(format!("{}-1", c.name)));
        let (b, _) = run_case(c, &base.join(format!("{}-2", c.name)));
        assert_eq!(a, b, "{}", c.name);
        let json = json.expect("正常の回と上限の回は result.json を書くはず");
        let items = a.items.unwrap();
        assert_eq!(
            json["result"]["items"]["pitch"]["q"], items.pitch.q,
            "{}",
            c.name
        );
        assert_eq!(
            json["result"]["items"]["timing"]["q"], items.timing.q,
            "{}",
            c.name
        );
        assert_eq!(
            json["result"]["items"]["duration"]["q"], items.duration.q,
            "{}",
            c.name
        );
        assert_eq!(json["result"]["passed"], a.passed.unwrap(), "{}", c.name);
    }
    let _ = std::fs::remove_dir_all(&base);
}

fn code(t: TimingResult) -> &'static str {
    match t {
        TimingResult::Within => "=",
        TimingResult::Early => "<",
        TimingResult::Late => ">",
        TimingResult::NotJudged => "-",
    }
}

/// AC-07：点数の控え（insta）。別のプロセスで流しても、控えと一致する。
#[test]
fn scores_match_the_snapshot() {
    let base = tmp("snapshot");
    let mut out = String::new();
    for c in CASES {
        let (r, _) = run_case(c, &base.join(c.name));
        let i = r.items.unwrap();
        let notes: Vec<String> = r
            .notes
            .iter()
            .map(|n| format!("{:?}{}{:?}", n.pitch, code(n.timing), n.duration))
            .collect();
        let _ = writeln!(
            out,
            "{} {:?}@{} q=({},{},{}) passed={} extras={} | {}",
            c.name,
            r.end.kind,
            r.end.at_ns,
            i.pitch.q,
            i.timing.q,
            i.duration.q,
            r.passed.unwrap(),
            r.extras.len(),
            notes.join(" ")
        );
    }
    insta::assert_snapshot!(out);
    let _ = std::fs::remove_dir_all(&base);
}
