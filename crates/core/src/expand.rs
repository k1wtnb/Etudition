//! 繰り返しを展開し、鳴る音（採点の1音）の列を作る（S1）。
//!
//! 位置の単位は四分音符の数、原点（0）は1小節目の頭。弱起の小節（1つ目の小節が `implicit="yes"`）の音は負の位置になる
//! （計画 2-3 の1）。

use crate::quarters::Quarters;
use crate::score::{Issue, Score, WrittenNoteId};

/// 展開した後の1つの小節。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedMeasure {
    /// 元の小節の文書の中の順番。
    pub measure_index: usize,
    /// この小節を弾くのが何回目か（1 始まり）。
    pub pass: u32,
    /// 展開した後の小節の頭の位置（四分音符の数。1小節目の頭が 0）。
    pub start: Quarters,
}

/// 鳴る音が含む、書かれた音符と、その音符を弾くのが何回目か。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenRef {
    /// 書かれた音符の ID。
    pub id: WrittenNoteId,
    /// その音符の小節を弾くのが何回目か（1 始まり）。
    pub pass: u32,
}

/// 鳴る音（採点の1音）。タイでつながった書かれた音符は1つにまとめる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundingNote {
    /// 音高（MIDI のノート番号）。
    pub pitch: i32,
    /// 始まりの位置（四分音符の数。1小節目の頭が 0）。
    pub start: Quarters,
    /// 長さ（四分音符の数）。装飾音は 0。
    pub duration: Quarters,
    /// 装飾音。
    pub is_grace: bool,
    /// もとの書かれた音符の列（タイの順）。
    pub written: Vec<WrittenRef>,
}

/// 展開の結果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Expansion {
    /// 展開した後の小節の列。
    pub measures: Vec<ExpandedMeasure>,
    /// 鳴る音の列。並びは「位置、装飾音が先、音高の低い順、楽譜の順」（計画 2-3 の4）。
    pub sounding: Vec<SoundingNote>,
    /// 展開で扱えなかったこと。
    pub issues: Vec<Issue>,
}

/// 小節の弾く順番（元の小節の番号の列）を決める。
fn measure_order(score: &Score, _issues: &mut Vec<Issue>) -> Vec<usize> {
    (0..score.measures.len()).collect()
}

/// 繰り返しを展開し、鳴る音の列を作る。
pub fn expand(score: &Score) -> Expansion {
    let mut issues = Vec::new();
    let order = measure_order(score, &mut issues);

    // 小節の頭の位置。弱起なら、1つ目の小節の頭を負にする
    let mut pos = match score.measures.first() {
        Some(m) if m.implicit => Quarters::ZERO - m.length,
        _ => Quarters::ZERO,
    };
    let mut passes = vec![0u32; score.measures.len()];
    let mut measures = Vec::new();
    for &i in &order {
        passes[i] += 1;
        measures.push(ExpandedMeasure {
            measure_index: i,
            pass: passes[i],
            start: pos,
        });
        pos = pos + score.measures[i].length;
    }

    // 書かれた音符を展開の順に並べ、位置の順（同じ位置なら楽譜の順）にしてから、タイをまとめる
    let mut events: Vec<(Quarters, usize, &ExpandedMeasure, usize)> = Vec::new();
    for (k, em) in measures.iter().enumerate() {
        for (j, n) in score.notes.iter().enumerate() {
            if n.id.measure_index == em.measure_index {
                events.push((em.start + n.offset, k, em, j));
            }
        }
    }
    events.sort_by_key(|&(start, k, _, j)| (start, k, j));

    // 並びのキー（位置、装飾音でない、音高、楽譜の順）と鳴る音
    let mut out: Vec<((Quarters, bool, i32, usize), SoundingNote)> = Vec::new();
    // タイの続きを待っている鳴る音（out の添字）
    let mut open: Vec<usize> = Vec::new();
    for (seq, &(start, _, em, j)) in events.iter().enumerate() {
        let n = &score.notes[j];
        let r = WrittenRef {
            id: n.id.clone(),
            pass: em.pass,
        };
        if n.tie_stop {
            let found = open.iter().position(|&o| {
                let s = &out[o].1;
                s.pitch == n.pitch && s.start + s.duration == start
            });
            if let Some(p) = found {
                let o = if n.tie_start { open[p] } else { open.remove(p) };
                let s = &mut out[o].1;
                s.duration = s.duration + n.duration;
                s.written.push(r);
                continue;
            }
            issues.push(Issue {
                measure_index: Some(n.id.measure_index),
                element: "tie".into(),
                detail: format!("タイの終わりに、つながる前の音がない（{}）", n.id),
            });
        }
        if n.tie_start {
            open.push(out.len());
        }
        out.push((
            (start, !n.is_grace, n.pitch, seq),
            SoundingNote {
                pitch: n.pitch,
                start,
                duration: n.duration,
                is_grace: n.is_grace,
                written: vec![r],
            },
        ));
    }
    for o in open {
        let s = &out[o].1;
        issues.push(Issue {
            measure_index: Some(s.written[0].id.measure_index),
            element: "tie".into(),
            detail: format!("タイの始まりに、続く音がない（{}）", s.written[0].id),
        });
    }
    out.sort_by_key(|(key, _)| *key);

    Expansion {
        measures,
        sounding: out.into_iter().map(|(_, s)| s).collect(),
        issues,
    }
}
