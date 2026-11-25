//! S1 の合成の楽譜（`fixtures/s1/`）を読み、書かれた音符と、展開後の鳴る音の列を照らす。
//! 期待する値は、楽譜を見て手で書いたもの（実装の出力を写したものではない）。

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
