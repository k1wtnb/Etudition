//! 採点（採点の仕様 v0 の3〜5章、7-4。境界の約束 B0 の 2-3〜2-5）。
//!
//! どの関数も純粋で、同じ入力なら同じ出力を返す（REQ-086、SR-13）。浮動小数点は使わない（計画 2-3 の4）。
//! 回の終わりの判定（`decide_end`）と、止まった回の採点（SR-22）は 1a-9 で足す。

use std::collections::BTreeMap;

use crate::expand::SoundingNote;
use crate::perf::{PerfEvent, PlayedNote, build_notes};
use crate::rules::Rules;
use crate::timing::{Timing, position_to_ns, span_to_ns};

/// 採点の関数への入力の全部。
#[derive(Debug, Clone, Copy)]
pub struct RunInput<'a> {
    /// お手本（展開済みの鳴る音の列。並びは SR-01）。
    pub reference: &'a [SoundingNote],
    /// 練習のテンポと、楽譜の位置 0 の時刻。
    pub timing: Timing,
    /// 演奏のイベント（補正後の時刻。受け取った順）。
    pub events: &'a [PerfEvent],
    /// 採点規則の版。
    pub rules: &'a Rules,
}

/// 回の終わり方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndKind {
    /// 正常に終わった（最後の音を離した）。
    Normal,
    /// 終わりの上限で終わった。
    Cap,
    /// 止まった。
    Stopped,
}

/// 回の終わり。`at_ns` は規則が決める終わりの時刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEnd {
    /// 終わり方。
    pub kind: EndKind,
    /// 終わりの時刻（ns）。
    pub at_ns: i64,
}

/// 音程の判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchResult {
    /// 正しい音高で組になった。
    Correct,
    /// 違う音高で組になった。
    WrongPitch,
    /// 組にならなかった（抜けた）。
    Missed,
}

/// タイミングの判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingResult {
    /// 許容幅の中。
    Within,
    /// 早い。
    Early,
    /// 遅い。
    Late,
    /// 判定しない（抜けた音）。
    NotJudged,
}

/// 音の長さの判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationResult {
    /// 許容幅の中。
    Within,
    /// 短い。
    Short,
    /// 長い。
    Long,
    /// 判定しない（抜けた音、装飾音）。
    NotJudged,
}

/// お手本の1音の判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteJudgement {
    /// `reference` の何番目か。
    pub ref_index: usize,
    /// 装飾音か。
    pub is_grace: bool,
    /// 音程。
    pub pitch: PitchResult,
    /// 組になった演奏の音。
    pub played: Option<PlayedNote>,
    /// タイミング。
    pub timing: TimingResult,
    /// 演奏のノートオンの時刻 − お手本の時刻（ns）。
    pub delta_ns: Option<i64>,
    /// 音の長さ。
    pub duration: DurationResult,
}

/// 1つの項目の点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemScore {
    /// 誤りの数。
    pub errors: u32,
    /// 分母。
    pub denominator: u32,
    /// q（100 点 ＝ `Rules::q_full`）。
    pub q: u32,
}

/// 3項目の点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Items {
    /// 音程。
    pub pitch: ItemScore,
    /// タイミング。
    pub timing: ItemScore,
    /// 音の長さ。
    pub duration: ItemScore,
}

/// 採点の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// 採点規則の版。
    pub rules_version: &'static str,
    /// 回の終わり。
    pub end: RunEnd,
    /// お手本の音ごとの判定（`reference` の順）。
    pub notes: Vec<NoteJudgement>,
    /// 余分な音（ノートオンの時刻の順）。
    pub extras: Vec<PlayedNote>,
    /// 3項目の点数。止まった回は `None`。
    pub items: Option<Items>,
    /// 合否。止まった回は `None`。
    pub passed: Option<bool>,
    /// 止まった位置の組の先頭の `ref_index`（1a-9 で入れる）。
    pub stopped_at: Option<usize>,
}

/// 演奏中の仮の印（SQ-04）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteMark {
    /// `reference` の何番目か。
    pub ref_index: usize,
    /// 音程。
    pub pitch: PitchResult,
    /// タイミング。
    pub timing: TimingResult,
}

/// 対応付けの結果。
struct Matching {
    /// お手本の音の時刻（ns）。
    ref_ns: Vec<i64>,
    /// 範囲に入った演奏の音（和音をまとめ直した後の順。SR-10）。
    played: Vec<PlayedNote>,
    /// お手本の音ごとに、組になった演奏の音（`played` の添字）。
    pair: Vec<Option<usize>>,
}

/// 範囲に入った演奏の音を作り、和音をまとめ直し（SR-09、SR-10）、動的計画法で対応付ける（SR-11、SR-12）。
/// `end_ns` より後のイベントは使わない。お手本は先頭の `ref_count` 個だけを使う（演奏中の印のため）。
fn match_notes(input: &RunInput, end_ns: i64, ref_count: usize) -> Matching {
    let rules = input.rules;
    let tempo = &input.timing.tempo;
    let w = span_to_ns(tempo, rules.match_window);
    let ref_ns: Vec<i64> = input.reference[..ref_count]
        .iter()
        .map(|r| position_to_ns(&input.timing, r.start))
        .collect();
    let range_start = ref_ns
        .iter()
        .min()
        .map_or(i64::MAX, |&e| e.saturating_sub(w));

    // SR-09：ノートオンが範囲の始まりより前の音は無視する（始まりちょうどは入る）
    let in_range: Vec<(PlayedNote, usize)> = build_notes(input.events, end_ns)
        .into_iter()
        .filter(|(p, _)| p.on_ns >= range_start)
        .collect();

    // SR-10：組の最初の音から CHORD_SPREAD 未満で来た音を同じ組にし、組の中を音高の順（同じなら受け取った順）にする
    let spread = span_to_ns(tempo, rules.chord_spread);
    let mut played: Vec<PlayedNote> = Vec::with_capacity(in_range.len());
    let mut k = 0;
    while k < in_range.len() {
        let first = in_range[k].0.on_ns;
        let mut end = k + 1;
        while end < in_range.len() && in_range[end].0.on_ns - first < spread {
            end += 1;
        }
        let mut group = in_range[k..end].to_vec();
        // 同じ音高は、ノートオンを受け取った順（時刻の順ではない）
        group.sort_by_key(|(n, seq)| (n.note, *seq));
        played.extend(group.into_iter().map(|(n, _)| n));
        k = end;
    }

    let pair = align(input, &ref_ns, &played, w);
    Matching {
        ref_ns,
        played,
        pair,
    }
}

/// 時刻の順を保つ動的計画法（SR-11）。同点は「組にする → お手本を飛ばす → 演奏を飛ばす」の順（SR-12）。
fn align(input: &RunInput, ref_ns: &[i64], played: &[PlayedNote], w: i64) -> Vec<Option<usize>> {
    let rules = input.rules;
    let gap = rules.cost_gap_windows * w;
    let mismatch = rules.cost_pitch_mismatch_windows * w;
    let (n, m) = (ref_ns.len(), played.len());
    let cols = m + 1;
    let pair_cost = |i: usize, j: usize| -> Option<i64> {
        let delta = played[j].on_ns - ref_ns[i];
        if delta.abs() > w {
            return None;
        }
        let same = i32::from(played[j].note) == input.reference[i].pitch;
        Some(delta.abs() + if same { 0 } else { mismatch })
    };
    let mut dp = vec![0_i64; (n + 1) * cols];
    for i in 0..=n {
        for j in 0..=m {
            let v = if i == 0 {
                j as i64 * gap
            } else if j == 0 {
                i as i64 * gap
            } else {
                let mut best = (dp[(i - 1) * cols + j] + gap).min(dp[i * cols + j - 1] + gap);
                if let Some(c) = pair_cost(i - 1, j - 1) {
                    best = best.min(dp[(i - 1) * cols + j - 1] + c);
                }
                best
            };
            dp[i * cols + j] = v;
        }
    }
    let mut pair = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let here = dp[i * cols + j];
        if i > 0
            && j > 0
            && let Some(c) = pair_cost(i - 1, j - 1)
            && here == dp[(i - 1) * cols + j - 1] + c
        {
            pair[i - 1] = Some(j - 1);
            i -= 1;
            j -= 1;
        } else if i > 0 && here == dp[(i - 1) * cols + j] + gap {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pair
}

/// タイミングの判定（SR-06：許容幅ちょうどは中）。
fn judge_timing(delta: i64, tolerance: i64) -> TimingResult {
    if delta.abs() <= tolerance {
        TimingResult::Within
    } else if delta < 0 {
        TimingResult::Early
    } else {
        TimingResult::Late
    }
}

/// 音の長さの判定（掛け算で比べる。整数だけ）。
fn judge_duration(played_len: i64, ref_len: i64, rules: &Rules) -> DurationResult {
    let (p, r) = (i128::from(played_len), i128::from(ref_len));
    let (min_n, min_d) = rules.duration_ratio_min;
    let (max_n, max_d) = rules.duration_ratio_max;
    if p * i128::from(min_d) < i128::from(min_n) * r {
        DurationResult::Short
    } else if p * i128::from(max_d) > i128::from(max_n) * r {
        DurationResult::Long
    } else {
        DurationResult::Within
    }
}

/// q ＝（`q_full` × 良い音の数）÷ 分母（切り捨て）。分母が 0 なら 0（SR-16）。
fn item_score(errors: u32, denominator: u32, rules: &Rules) -> ItemScore {
    let good = denominator.saturating_sub(errors);
    let q = if denominator == 0 {
        0
    } else {
        u32::try_from(u64::from(rules.q_full) * u64::from(good) / u64::from(denominator))
            .unwrap_or(u32::MAX)
    };
    ItemScore {
        errors,
        denominator,
        q,
    }
}

/// 合否（SR-18）：音程と音の長さの q ÷ `q_per_point`（整数の割り算）が、どちらも合格の線以上。
fn is_passed(items: &Items, rules: &Rules) -> bool {
    items.pitch.q / rules.q_per_point >= rules.pass_line_pitch
        && items.duration.q / rules.q_per_point >= rules.pass_line_duration
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 終わった回を採点する（SR-14〜SR-19）。`end.at_ns` より後のノートオンは捨てる。
///
/// 止まった回（`EndKind::Stopped`）は q と合否を出さない。止まった位置より前に絞る採点（SR-22）は 1a-9 で足す。
pub fn score_run(input: &RunInput, end: RunEnd) -> RunResult {
    let rules = input.rules;
    let tempo = &input.timing.tempo;
    let tolerance = span_to_ns(tempo, rules.timing_tolerance);
    let m = match_notes(input, end.at_ns, input.reference.len());

    let mut used = vec![false; m.played.len()];
    let mut notes = Vec::with_capacity(input.reference.len());
    for (i, r) in input.reference.iter().enumerate() {
        let judged = match m.pair[i] {
            None => NoteJudgement {
                ref_index: i,
                is_grace: r.is_grace,
                pitch: PitchResult::Missed,
                played: None,
                timing: TimingResult::NotJudged,
                delta_ns: None,
                duration: DurationResult::NotJudged,
            },
            Some(j) => {
                used[j] = true;
                let p = m.played[j];
                let delta = p.on_ns - m.ref_ns[i];
                let duration = if r.is_grace && !rules.grace_duration_scored {
                    DurationResult::NotJudged
                } else {
                    judge_duration(p.off_ns - p.on_ns, span_to_ns(tempo, r.duration), rules)
                };
                NoteJudgement {
                    ref_index: i,
                    is_grace: r.is_grace,
                    pitch: if i32::from(p.note) == r.pitch {
                        PitchResult::Correct
                    } else {
                        PitchResult::WrongPitch
                    },
                    played: Some(p),
                    timing: judge_timing(delta, tolerance),
                    delta_ns: Some(delta),
                    duration,
                }
            }
        };
        notes.push(judged);
    }
    let mut extras: Vec<PlayedNote> = m
        .played
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(p, _)| *p)
        .collect();
    extras.sort_by_key(|p| p.on_ns);

    let (items, passed) = if end.kind == EndKind::Stopped {
        (None, None)
    } else {
        let paired: Vec<&NoteJudgement> = notes.iter().filter(|n| n.played.is_some()).collect();
        let pitch_errors = notes
            .iter()
            .filter(|n| n.pitch != PitchResult::Correct)
            .count()
            + extras.len();
        let timing_errors = paired
            .iter()
            .filter(|n| !matches!(n.timing, TimingResult::Within))
            .count();
        let duration_judged = paired
            .iter()
            .filter(|n| n.duration != DurationResult::NotJudged)
            .count();
        let duration_errors = paired
            .iter()
            .filter(|n| matches!(n.duration, DurationResult::Short | DurationResult::Long))
            .count();
        let items = Items {
            pitch: item_score(count(pitch_errors), count(notes.len()), rules),
            timing: item_score(count(timing_errors), count(paired.len()), rules),
            duration: item_score(count(duration_errors), count(duration_judged), rules),
        };
        (Some(items), Some(is_passed(&items, rules)))
    };

    RunResult {
        rules_version: rules.version,
        end,
        notes,
        extras,
        items,
        passed,
        stopped_at: None,
    }
}

/// 演奏中の仮の印（SQ-04、計画6章 #12。境界の約束 B0 の 2-4）。
///
/// 窓の閉じる時刻（お手本の時刻 ＋ `match_window`）が `after_ns` より後で `until_ns` 以下のお手本の音について、
/// その音の窓の閉じる時刻までの入力と、その時刻までに窓が閉じたお手本の音（`reference` の先頭から）だけで対応付けた印を返す。
/// まだ窓が閉じていない後の音には、組を取らせない。並びは窓の閉じる時刻の順、同じなら `reference` の順。
pub fn marks_closed_between(input: &RunInput, after_ns: i64, until_ns: i64) -> Vec<NoteMark> {
    let tempo = &input.timing.tempo;
    let w = span_to_ns(tempo, input.rules.match_window);
    let tolerance = span_to_ns(tempo, input.rules.timing_tolerance);
    let closes: Vec<i64> = input
        .reference
        .iter()
        .map(|r| position_to_ns(&input.timing, r.start).saturating_add(w))
        .collect();
    let mut by_close: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, &close) in closes.iter().enumerate() {
        if close > after_ns && close <= until_ns {
            by_close.entry(close).or_default().push(i);
        }
    }
    let mut marks = Vec::new();
    for (close, refs) in by_close {
        // お手本は位置の順（SR-01）なので、窓が閉じた音は先頭からの並びになる
        let ref_count = closes.iter().take_while(|&&c| c <= close).count();
        let m = match_notes(input, close, ref_count);
        for i in refs {
            let mark = match m.pair[i] {
                None => NoteMark {
                    ref_index: i,
                    pitch: PitchResult::Missed,
                    timing: TimingResult::NotJudged,
                },
                Some(j) => NoteMark {
                    ref_index: i,
                    pitch: if i32::from(m.played[j].note) == input.reference[i].pitch {
                        PitchResult::Correct
                    } else {
                        PitchResult::WrongPitch
                    },
                    timing: judge_timing(m.played[j].on_ns - m.ref_ns[i], tolerance),
                },
            };
            marks.push(mark);
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RULES_V0;

    /// AC-08、AC-40 の境目：q と合否。
    #[test]
    fn q_and_pass_boundaries() {
        let r = &RULES_V0;
        assert_eq!(item_score(1, 10, r).q, 900_000);
        assert_eq!(item_score(1, 3, r).q, 666_666);
        assert_eq!(item_score(3, 10, r).q, 700_000);
        assert_eq!(item_score(0, 0, r).q, 0);
        assert_eq!(item_score(12, 10, r).q, 0);
        let items = |p: u32, d: u32| Items {
            pitch: ItemScore {
                errors: 0,
                denominator: 1,
                q: p,
            },
            timing: ItemScore {
                errors: 0,
                denominator: 0,
                q: 0,
            },
            duration: ItemScore {
                errors: 0,
                denominator: 1,
                q: d,
            },
        };
        assert!(!is_passed(&items(899_999, 1_000_000), r));
        assert!(is_passed(&items(900_000, 1_000_000), r));
        assert!(is_passed(&items(907_000, 1_000_000), r));
        assert!(!is_passed(&items(1_000_000, 699_999), r));
        assert!(is_passed(&items(1_000_000, 700_000), r));
        // 対応が付いた音が 0 なら音の長さは 0 点で不合格
        assert!(!is_passed(&items(1_000_000, item_score(0, 0, r).q), r));
    }

    /// 音の長さの境界（9-3）：比 0.5 と 1.5 ちょうどは中。
    #[test]
    fn duration_boundaries() {
        let r = &RULES_V0;
        assert_eq!(judge_duration(400, 1000, r), DurationResult::Short);
        assert_eq!(judge_duration(500, 1000, r), DurationResult::Within);
        assert_eq!(judge_duration(1500, 1000, r), DurationResult::Within);
        assert_eq!(judge_duration(1600, 1000, r), DurationResult::Long);
    }

    /// タイミングの境界（SR-06）：許容幅ちょうどは中。
    #[test]
    fn timing_boundaries() {
        assert_eq!(judge_timing(125, 125), TimingResult::Within);
        assert_eq!(judge_timing(-125, 125), TimingResult::Within);
        assert_eq!(judge_timing(126, 125), TimingResult::Late);
        assert_eq!(judge_timing(-126, 125), TimingResult::Early);
    }
}
