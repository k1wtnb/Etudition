//! S1 の合成の楽譜（`fixtures/s1/`）を読み、書かれた音符と、展開後の鳴る音の列を照らす。
//! 期待する値は、楽譜を見て手で書いたもの（実装の出力を写したものではない）。

use etudition_core::expand::expand;
use etudition_core::score::{Score, read_score};

/// 楽譜を読み、読めない要素がないことを確かめて返す。
fn load(bytes: &[u8]) -> Score {
    let score = read_score(bytes).expect("読めるはず").score;
    assert!(score.issues.is_empty(), "読めない要素: {:?}", score.issues);
    score
}

/// 書かれた音符を「ID 譜表 音高 小節の中の位置+長さ 印」の文字列にする（位置と長さは四分音符の数）。
fn written(score: &Score) -> Vec<String> {
    score
        .notes
        .iter()
        .map(|n| {
            let mut s = format!(
                "{} s{} {} {}+{}",
                n.id, n.staff, n.pitch, n.offset, n.duration
            );
            if n.tie_start {
                s.push_str(" tie>");
            }
            if n.tie_stop {
                s.push_str(" >tie");
            }
            if n.is_grace {
                s.push_str(" grace");
            }
            s
        })
        .collect()
}

#[test]
fn tie_written_notes() {
    let score = load(include_bytes!("../../../fixtures/s1/01-tie.musicxml"));
    assert_eq!(
        written(&score),
        [
            "P1/m0/v1/n0 s1 60 0+2",
            "P1/m0/v1/n1 s1 64 2+2 tie>",
            "P1/m1/v1/n0 s1 64 0+2 >tie",
            "P1/m1/v1/n1 s1 67 2+2",
        ]
    );
}

/// 展開した小節を「m小節#何回目@位置」の列にし、鳴る音を「音高@位置+長さ [ID#何回目 ...]」の列にする。
fn expanded(score: &Score) -> (Vec<String>, Vec<String>) {
    let e = expand(score);
    assert!(e.issues.is_empty(), "展開の問題: {:?}", e.issues);
    let measures = e
        .measures
        .iter()
        .map(|m| format!("m{}#{}@{}", m.measure_index, m.pass, m.start))
        .collect();
    let sounding = e
        .sounding
        .iter()
        .map(|n| {
            let refs: Vec<String> = n
                .written
                .iter()
                .map(|w| format!("{}#{}", w.id, w.pass))
                .collect();
            let grace = if n.is_grace { " grace" } else { "" };
            format!(
                "{}@{}+{} [{}]{}",
                n.pitch,
                n.start,
                n.duration,
                refs.join(" "),
                grace
            )
        })
        .collect();
    (measures, sounding)
}

#[test]
fn tie_sounding_notes() {
    let score = load(include_bytes!("../../../fixtures/s1/01-tie.musicxml"));
    let (measures, sounding) = expanded(&score);
    assert_eq!(measures, ["m0#1@0", "m1#1@4"]);
    assert_eq!(
        sounding,
        [
            "60@0+2 [P1/m0/v1/n0#1]",
            "64@2+4 [P1/m0/v1/n1#1 P1/m1/v1/n0#1]",
            "67@6+2 [P1/m1/v1/n1#1]",
        ]
    );
}

#[test]
fn chord() {
    let score = load(include_bytes!("../../../fixtures/s1/02-chord.musicxml"));
    assert_eq!(
        written(&score),
        [
            "P1/m0/v1/n0 s1 67 0+2",
            "P1/m0/v1/n1 s1 60 0+2",
            "P1/m0/v1/n2 s1 64 0+2",
            "P1/m0/v1/n3 s1 65 2+2",
            "P1/m0/v1/n4 s1 62 2+2",
        ]
    );
    let (measures, sounding) = expanded(&score);
    assert_eq!(measures, ["m0#1@0"]);
    assert_eq!(
        sounding,
        [
            "60@0+2 [P1/m0/v1/n1#1]",
            "64@0+2 [P1/m0/v1/n2#1]",
            "67@0+2 [P1/m0/v1/n0#1]",
            "62@2+2 [P1/m0/v1/n4#1]",
            "65@2+2 [P1/m0/v1/n3#1]",
        ]
    );
}

#[test]
fn two_voices() {
    let score = load(include_bytes!(
        "../../../fixtures/s1/03-two-voices.musicxml"
    ));
    let (_, sounding) = expanded(&score);
    assert_eq!(
        sounding,
        [
            "72@0+2 [P1/m0/v2/n0#1]",
            "76@0+1 [P1/m0/v1/n0#1]",
            "76@1+1 [P1/m0/v1/n1#1]",
            "69@2+2 [P1/m0/v2/n1#1]",
            "76@2+1 [P1/m0/v1/n2#1]",
            "76@3+1 [P1/m0/v1/n3#1]",
        ]
    );
}

#[test]
fn grand_staff() {
    let score = load(include_bytes!(
        "../../../fixtures/s1/04-grand-staff.musicxml"
    ));
    assert_eq!(
        written(&score),
        [
            "P1/m0/v1/n0 s1 72 0+4",
            "P1/m0/v5/n0 s2 48 0+2",
            "P1/m0/v5/n1 s2 55 2+2",
        ]
    );
    let (_, sounding) = expanded(&score);
    assert_eq!(
        sounding,
        [
            "48@0+2 [P1/m0/v5/n0#1]",
            "72@0+4 [P1/m0/v1/n0#1]",
            "55@2+2 [P1/m0/v5/n1#1]",
        ]
    );
}

#[test]
fn pickup() {
    let score = load(include_bytes!("../../../fixtures/s1/07-pickup.musicxml"));
    let (measures, sounding) = expanded(&score);
    assert_eq!(measures, ["m0#1@-1", "m1#1@0", "m2#1@4"]);
    assert_eq!(
        sounding,
        [
            "67@-1+1 [P1/m0/v1/n0#1]",
            "72@0+4 [P1/m1/v1/n0#1]",
            "76@4+2 [P1/m2/v1/n0#1]",
        ]
    );
}

#[test]
fn grace() {
    let score = load(include_bytes!("../../../fixtures/s1/08-grace.musicxml"));
    assert_eq!(
        written(&score),
        [
            "P1/m0/v1/n0 s1 74 0+0 grace",
            "P1/m0/v1/n1 s1 72 0+1",
            "P1/m0/v1/n2 s1 76 1+1",
            "P1/m0/v1/n3 s1 79 2+2",
        ]
    );
    let (_, sounding) = expanded(&score);
    assert_eq!(
        sounding,
        [
            "74@0+0 [P1/m0/v1/n0#1] grace",
            "72@0+1 [P1/m0/v1/n1#1]",
            "76@1+1 [P1/m0/v1/n2#1]",
            "79@2+2 [P1/m0/v1/n3#1]",
        ]
    );
}

#[test]
fn repeat_and_volta() {
    let score = load(include_bytes!(
        "../../../fixtures/s1/05-repeat-volta.musicxml"
    ));
    let (measures, sounding) = expanded(&score);
    assert_eq!(
        measures,
        [
            "m0#1@0", "m1#1@4", "m2#1@8", "m0#2@12", "m1#2@16", "m3#1@20", "m4#1@24",
        ]
    );
    assert_eq!(
        sounding,
        [
            "60@0+4 [P1/m0/v1/n0#1]",
            "62@4+4 [P1/m1/v1/n0#1]",
            "64@8+4 [P1/m2/v1/n0#1]",
            "60@12+4 [P1/m0/v1/n0#2]",
            "62@16+4 [P1/m1/v1/n0#2]",
            "65@20+4 [P1/m3/v1/n0#1]",
            "67@24+4 [P1/m4/v1/n0#1]",
        ]
    );
}
