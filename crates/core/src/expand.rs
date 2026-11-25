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
    /// 鳴る音の列（お手本）。並びは「位置、音高の低い順、楽譜の順」（計画 2-3 の4、採点の仕様 v0 の SR-01）。
    /// 装飾音も同じ位置の音として音高の順に並ぶ（S1 の「装飾音が先」は 1a-1 でやめた）。
    pub sounding: Vec<SoundingNote>,
    /// 読み込み（[`Score::issues`]）と展開で扱えなかったこと。1つでもあれば [`check_playable`] が止める。
    pub issues: Vec<Issue>,
}

/// 小節の弾く順番（元の小節の番号の列）を決める。
///
/// 扱う記号：前向き・後ろ向きの繰り返し記号（`times`）、かっこ（番号の一覧）、
/// D.S.（`dalsegno` で `segno` へ戻る。1回だけ）と、D.S. の後の To Coda（`tocoda` で `coda` へ跳ぶ）。
/// D.S. の後に出てくる繰り返し記号とかっこは扱わず、問題として返す（慣習が割れるため。試験の楽譜にない）。
/// 展開の歩数が「小節の数 × 上限」を超えたら、止めて問題として返す（REQ-105）。
fn measure_order(score: &Score, issues: &mut Vec<Issue>) -> Vec<usize> {
    const MAX_STEPS_PER_MEASURE: usize = 16;
    let ms = &score.measures;
    let n = ms.len();
    let mut order = Vec::new();
    let mut i = 0;
    // 今の繰り返しの部分の頭と、何周目か
    let mut repeat_start = 0;
    let mut round = 1u32;
    let mut jumped_back = false;
    let mut after_ds = false;
    let mut issue = |i: usize, element: &str, detail: &str| {
        issues.push(Issue {
            measure_index: Some(i),
            element: element.into(),
            detail: detail.into(),
        })
    };
    let find = |label: &String, f: fn(&crate::score::Measure) -> &Option<String>| {
        ms.iter().position(|m| f(m).as_ref() == Some(label))
    };
    while i < n {
        if order.len() > n * MAX_STEPS_PER_MEASURE {
            issue(i, "repeat", "繰り返しの展開が終わらない");
            break;
        }
        let m = &ms[i];
        if after_ds && (m.repeat_forward || m.repeat_backward.is_some() || m.ending_start.is_some())
        {
            issue(
                i,
                "repeat",
                "D.S. の後の繰り返し記号・かっこは扱わない（無視した）",
            );
            order.push(i);
            i += 1;
            continue;
        }
        if m.repeat_forward && !jumped_back {
            repeat_start = i;
            round = 1;
        }
        jumped_back = false;
        // 今の周の番号を含まないかっこは、かっこの終わりまで飛ばす
        if let Some(nums) = &m.ending_start
            && !nums.contains(&round)
        {
            let mut j = i;
            while j < n && !ms[j].ending_end {
                j += 1;
            }
            i = j + 1;
            continue;
        }
        order.push(i);
        if after_ds && let Some(label) = &m.tocoda {
            match find(label, |m| &m.coda) {
                Some(c) => {
                    i = c;
                    continue;
                }
                None => issue(i, "sound@tocoda", "跳び先の coda がない"),
            }
        }
        if !after_ds && let Some(label) = &m.dalsegno {
            match find(label, |m| &m.segno) {
                Some(sg) => {
                    after_ds = true;
                    i = sg;
                    continue;
                }
                None => issue(i, "sound@dalsegno", "戻り先の segno がない"),
            }
        }
        if let Some(times) = m.repeat_backward {
            if round < times {
                round += 1;
                i = repeat_start;
                jumped_back = true;
                continue;
            }
            repeat_start = i + 1;
            round = 1;
        }
        i += 1;
    }
    order
}

/// 繰り返しを展開し、鳴る音の列を作る。
///
/// 読み込みの問題（[`Score::issues`]）も、展開の問題の前に [`Expansion::issues`] へ写す。
pub fn expand(score: &Score) -> Expansion {
    let mut issues = score.issues.clone();
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

    // 並びのキー（位置、音高、楽譜の順。SR-01）と鳴る音
    let mut out: Vec<((Quarters, i32, usize), SoundingNote)> = Vec::new();
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
            (start, n.pitch, seq),
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

/// 採点に使える楽譜かを確かめる（B07、REQ-105。境界の約束 B0 の 2-1）。
///
/// 読めない要素、パートが2つ以上、終わらない繰り返し、タイの食い違いなど、[`Expansion::issues`] が1つでもあるとき、
/// またはお手本の音が0個のときは、理由の一覧を返して止める。黙って捨てて先へ進まない（AC-03、AC-45）。
pub fn check_playable(expansion: &Expansion) -> Result<(), Vec<Issue>> {
    let mut issues = expansion.issues.clone();
    if expansion.sounding.is_empty() {
        issues.push(Issue {
            measure_index: None,
            element: "note".into(),
            detail: "音高のある音符が0個（採点できない）".into(),
        });
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}
