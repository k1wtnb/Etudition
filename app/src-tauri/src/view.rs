//! 画面に渡す型（境界の約束 B0 の5章）。時刻はすべて `UiTime`（P0-02）。ts-rs で TypeScript の型を作る。
//!
//! 型は `cargo test -p etudition-app` のときに `app/ui/src/lib/bindings/` へ書き出される（ts-rs）。

use etudition_core::expand::SoundingNote;
use etudition_core::perf::PlayedNote;
use etudition_core::quarters::Quarters;
use etudition_core::score::WrittenNote;
use etudition_core::scoring::{
    DurationResult, EndKind, ItemScore, NoteMark, PitchResult, RunResult, TimingResult,
};
use etudition_engine::record::RecordStatus;
use etudition_engine::session::{EngineEvent, InvalidReason};
use serde::Serialize;
use ts_rs::TS;

use crate::ui_time::UiTime;

/// 練習のテンポ（四分音符／分 ＝ `num / den`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct TempoView {
    /// 分子。
    pub num: u32,
    /// 分母。
    pub den: u32,
}

/// お手本の1音が含む、書かれた音符と何回目か。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct WrittenView {
    /// 書かれた音符の ID（`P1/m0/v1/n0`）。
    pub id: String,
    /// 何回目か（1 始まり）。
    pub pass: u32,
}

/// 四分音符の数（`num / den`。画面では表示とカーソルにだけ使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct QuartersView {
    /// 分子（弱起は負）。
    pub num: i32,
    /// 分母。
    pub den: u32,
}

fn quarters(q: Quarters) -> Result<QuartersView, String> {
    Ok(QuartersView {
        num: i32::try_from(q.num()).map_err(|_| format!("画面に渡せない位置: {q:?}"))?,
        den: u32::try_from(q.den()).map_err(|_| format!("画面に渡せない位置: {q:?}"))?,
    })
}

/// 楽譜に書かれた1つの音符。画面は、OSMD の音符と、小節・譜表・声部・小節の中の位置・音高・装飾音かで突き合わせる（S1 の1段目）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct WrittenNoteView {
    /// 書かれた音符の ID（`P1/m0/v1/n0`）。
    pub id: String,
    /// 小節の文書の中の順番（0 始まり）。
    pub measure_index: u32,
    /// `voice` 要素の文字列。
    pub voice: String,
    /// 譜表の番号（1 始まり）。
    pub staff: u32,
    /// 音高（MIDI のノート番号）。
    pub pitch: i32,
    /// 小節の頭からの位置。
    pub offset_quarters: QuartersView,
    /// 装飾音か。
    pub is_grace: bool,
}

/// お手本の1音。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct RefNoteView {
    /// お手本の何番目か。
    pub ref_index: u32,
    /// 始まりの位置（1小節目の頭が 0。カーソルを進める時機に使う）。
    pub start_quarters: QuartersView,
    /// 音高（MIDI のノート番号）。
    pub pitch: i32,
    /// 装飾音か。
    pub is_grace: bool,
    /// 書かれた音符の列。画面は ID で OSMD の音符を探す（S1 の2段の対応）。
    pub written: Vec<WrittenView>,
}

/// 読み込んだ楽譜。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct ScoreView {
    /// 楽譜の XML の文字列（OSMD に渡す）。
    pub xml: String,
    /// 書かれた音符（文書の順）。
    pub written: Vec<WrittenNoteView>,
    /// お手本の音（並びは採点の仕様 v0 の SR-01）。
    pub notes: Vec<RefNoteView>,
    /// 音楽の拍（四分音符の数）。画面はテンポをこの音符で示し、入れる（楽譜の慣例。Q-29）。
    pub beat: QuartersView,
}

impl ScoreView {
    /// 書かれた音符とお手本から作る。位置が i32 と u32 に収まらない楽譜は誤りにする。
    pub fn new(
        xml: String,
        written: &[WrittenNote],
        sounding: &[SoundingNote],
        beat: Quarters,
    ) -> Result<ScoreView, String> {
        let written = written
            .iter()
            .map(|w| {
                Ok(WrittenNoteView {
                    id: w.id.to_string(),
                    measure_index: count(w.id.measure_index),
                    voice: w.id.voice.clone(),
                    staff: w.staff,
                    pitch: w.pitch,
                    offset_quarters: quarters(w.offset)?,
                    is_grace: w.is_grace,
                })
            })
            .collect::<Result<_, String>>()?;
        let notes = sounding
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Ok(RefNoteView {
                    ref_index: count(i),
                    start_quarters: quarters(s.start)?,
                    pitch: s.pitch,
                    is_grace: s.is_grace,
                    written: s
                        .written
                        .iter()
                        .map(|w| WrittenView {
                            id: w.id.to_string(),
                            pass: w.pass,
                        })
                        .collect(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(ScoreView {
            xml,
            written,
            notes,
            beat: quarters(beat)?,
        })
    }
}

/// 音程の判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum PitchView {
    Correct,
    WrongPitch,
    Missed,
}

/// タイミングの判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum TimingView {
    Within,
    Early,
    Late,
    NotJudged,
}

/// 音の長さの判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum DurationView {
    Within,
    Short,
    Long,
    NotJudged,
}

/// 回の終わり方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum EndView {
    Normal,
    Cap,
    Stopped,
}

/// 演奏中の仮の印。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct MarkView {
    pub ref_index: u32,
    pub pitch: PitchView,
    pub timing: TimingView,
}

/// 演奏の1音（打鍵の時刻）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct PlayedView {
    pub note: u8,
    pub on: UiTime,
    pub off: UiTime,
}

fn played(p: &PlayedNote) -> Result<PlayedView, String> {
    Ok(PlayedView {
        note: p.note,
        on: UiTime::new(p.on_ns)?,
        off: UiTime::new(p.off_ns)?,
    })
}

/// お手本の1音の、演奏後の判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct NoteView {
    pub ref_index: u32,
    pub is_grace: bool,
    pub pitch: PitchView,
    /// 組になった演奏の音。
    pub played: Option<PlayedView>,
    pub timing: TimingView,
    /// 演奏のノートオンの時刻 − お手本の時刻。
    pub delta: Option<UiTime>,
    pub duration: DurationView,
}

/// 1つの項目の点数（q は 100 点 ＝ 1,000,000。表示の形は画面で作る。SR-17）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct ItemView {
    pub errors: u32,
    pub denominator: u32,
    pub q: u32,
}

/// 3項目の点数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct ItemsView {
    pub pitch: ItemView,
    pub timing: ItemView,
    pub duration: ItemView,
}

/// 採点の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct RunResultView {
    pub rules_version: String,
    pub end: EndView,
    pub end_at: UiTime,
    /// お手本の音ごとの判定（止まった回は、止まった位置より前の音だけ）。
    pub notes: Vec<NoteView>,
    /// 余分な音。
    pub extras: Vec<PlayedView>,
    /// 止まった回は `null`。
    pub items: Option<ItemsView>,
    /// 止まった回は `null`。
    pub passed: Option<bool>,
    /// 止まった位置の組の先頭の `ref_index`。
    pub stopped_at: Option<u32>,
}

/// 記録の状態。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum RecordView {
    Written,
    /// 「記録できなかった」の知らせ（REQ-102）。
    Failed {
        message: String,
    },
    NotWritten,
}

/// 画面への通知（境界の約束 B0 の5章。`EngineEvent` の時刻を `UiTime` にしたもの）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum LiveEvent {
    /// 回が始まった。カーソルは `origin`（楽譜の位置 0 の時刻）と `tempo` で進める。
    Started { origin: UiTime, tempo: TempoView },
    /// クリック（1a-5）。
    Click {
        index: u32,
        count_in: bool,
        at: UiTime,
    },
    /// ノートオンを受けた（1a-8 の受けた1音の表示）。
    NoteReceived { note: u8, at: UiTime },
    /// 演奏中の仮の印。
    Marks { marks: Vec<MarkView> },
    /// 回が終わった。
    Ended {
        result: RunResultView,
        record: RecordView,
    },
    /// 回を無効にした。画面は理由と、次にすることの案内を出す（Q-52 の C。人間の判断 2026-11-26）。
    Invalid {
        reason: String,
        advice: String,
        record: RecordView,
    },
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn pitch(p: PitchResult) -> PitchView {
    match p {
        PitchResult::Correct => PitchView::Correct,
        PitchResult::WrongPitch => PitchView::WrongPitch,
        PitchResult::Missed => PitchView::Missed,
    }
}

fn timing(t: TimingResult) -> TimingView {
    match t {
        TimingResult::Within => TimingView::Within,
        TimingResult::Early => TimingView::Early,
        TimingResult::Late => TimingView::Late,
        TimingResult::NotJudged => TimingView::NotJudged,
    }
}

fn duration(d: DurationResult) -> DurationView {
    match d {
        DurationResult::Within => DurationView::Within,
        DurationResult::Short => DurationView::Short,
        DurationResult::Long => DurationView::Long,
        DurationResult::NotJudged => DurationView::NotJudged,
    }
}

fn item(i: ItemScore) -> ItemView {
    ItemView {
        errors: i.errors,
        denominator: i.denominator,
        q: i.q,
    }
}

fn mark(m: &NoteMark) -> MarkView {
    MarkView {
        ref_index: count(m.ref_index),
        pitch: pitch(m.pitch),
        timing: timing(m.timing),
    }
}

fn record(r: RecordStatus) -> RecordView {
    match r {
        RecordStatus::Written(_) => RecordView::Written,
        RecordStatus::Failed(message) => RecordView::Failed { message },
        RecordStatus::NotWritten => RecordView::NotWritten,
    }
}

/// 無効の回で、次にすることの案内（Q-52 の C）。
fn advice(r: InvalidReason) -> String {
    match r {
        InvalidReason::Disconnected => {
            "MIDI の機器の接続（ケーブルと電源）を確かめて、もう一度始めてください。".into()
        }
        InvalidReason::Overflow { .. } => {
            "音を受け取りきれませんでした。ほかのアプリを閉じてから、もう一度始めてください。"
                .into()
        }
        InvalidReason::AudioLost => {
            "音を出す機器（スピーカーやイヤホン）の接続を確かめて、もう一度始めてください。".into()
        }
    }
}

fn reason(r: InvalidReason) -> String {
    match r {
        InvalidReason::Disconnected => "MIDI の接続が切れた".into(),
        InvalidReason::Overflow { count } => format!("受信のあふれ（{count} 件）"),
        InvalidReason::AudioLost => "音声の出力先がなくなった".into(),
    }
}

impl RunResultView {
    /// 採点の結果から作る。時刻が画面に渡せる範囲の外なら誤り。
    pub fn new(r: &RunResult) -> Result<RunResultView, String> {
        let notes = r
            .notes
            .iter()
            .map(|n| {
                Ok(NoteView {
                    ref_index: count(n.ref_index),
                    is_grace: n.is_grace,
                    pitch: pitch(n.pitch),
                    played: n.played.as_ref().map(played).transpose()?,
                    timing: timing(n.timing),
                    delta: n.delta_ns.map(UiTime::new).transpose()?,
                    duration: duration(n.duration),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(RunResultView {
            rules_version: r.rules_version.to_string(),
            end: match r.end.kind {
                EndKind::Normal => EndView::Normal,
                EndKind::Cap => EndView::Cap,
                EndKind::Stopped => EndView::Stopped,
            },
            end_at: UiTime::new(r.end.at_ns)?,
            notes,
            extras: r
                .extras
                .iter()
                .map(played)
                .collect::<Result<Vec<_>, String>>()?,
            items: r.items.map(|i| ItemsView {
                pitch: item(i.pitch),
                timing: item(i.timing),
                duration: item(i.duration),
            }),
            passed: r.passed,
            stopped_at: r.stopped_at.map(count),
        })
    }
}

impl LiveEvent {
    /// `EngineEvent` を画面への通知にする。時刻が画面に渡せる範囲の外なら誤り（丸めない。P0-02）。
    pub fn from_engine(ev: EngineEvent) -> Result<LiveEvent, String> {
        Ok(match ev {
            EngineEvent::Started { timing: t } => LiveEvent::Started {
                origin: UiTime::new(t.origin_ns)?,
                tempo: TempoView {
                    num: t.tempo.num(),
                    den: t.tempo.den(),
                },
            },
            EngineEvent::Click {
                index,
                count_in,
                scheduled_ns,
            } => LiveEvent::Click {
                index,
                count_in,
                at: UiTime::new(scheduled_ns)?,
            },
            EngineEvent::NoteReceived { note, at_ns } => LiveEvent::NoteReceived {
                note,
                at: UiTime::new(at_ns)?,
            },
            EngineEvent::Marks(marks) => LiveEvent::Marks {
                marks: marks.iter().map(mark).collect(),
            },
            EngineEvent::Ended { result, record: r } => LiveEvent::Ended {
                result: RunResultView::new(&result)?,
                record: record(r),
            },
            EngineEvent::Invalid {
                reason: why,
                record: r,
            } => LiveEvent::Invalid {
                reason: reason(why),
                advice: advice(why),
                record: record(r),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 無効の回は、理由と、次にすることの案内を画面に渡す（Q-52 の C。人間の判断 2026-11-26）。
    #[test]
    fn invalid_event_carries_reason_and_advice() {
        let cases = [
            (
                InvalidReason::Disconnected,
                "MIDI の接続が切れた",
                "MIDI の機器",
            ),
            (
                InvalidReason::Overflow { count: 3 },
                "受信のあふれ（3 件）",
                "ほかのアプリ",
            ),
            (
                InvalidReason::AudioLost,
                "音声の出力先がなくなった",
                "音を出す機器",
            ),
        ];
        for (why, text, hint) in cases {
            let ev = LiveEvent::from_engine(EngineEvent::Invalid {
                reason: why,
                record: RecordStatus::NotWritten,
            })
            .unwrap();
            let LiveEvent::Invalid { reason, advice, .. } = ev else {
                panic!("Invalid でない");
            };
            assert_eq!(reason, text);
            assert!(advice.contains(hint), "{advice}");
            assert!(advice.contains("もう一度始めて"), "{advice}");
        }
    }

    #[test]
    fn live_event_is_tagged_and_times_are_numbers() {
        let ev = LiveEvent::NoteReceived {
            note: 60,
            at: UiTime::new(3_000_000_000).unwrap(),
        };
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"type":"note_received","note":60,"at":{"ns":3000000000}}"#
        );
        let decl = LiveEvent::decl(&ts_rs::Config::default());
        assert!(decl.contains("\"type\": \"note_received\""), "{decl}");
    }

    /// 画面は、書かれた音符の中身で OSMD の音符を探し、お手本の音の位置でカーソルを進める（1a-7）。
    #[test]
    fn score_view_has_written_notes_and_positions() {
        let loaded = etudition_core::score::read_score(include_bytes!(
            "../../../fixtures/s1/07-pickup.musicxml"
        ))
        .unwrap();
        let e = etudition_core::expand::expand(&loaded.score);
        let v = ScoreView::new(
            String::new(),
            &loaded.score.notes,
            &e.sounding,
            Quarters::new(1, 1),
        )
        .unwrap();
        assert_eq!(v.written.len(), loaded.score.notes.len());
        assert_eq!(v.written[0].id, "P1/m0/v1/n0");
        assert_eq!(v.notes.len(), e.sounding.len());
        // 弱起の音は 0 より前
        assert!(v.notes[0].start_quarters.num < 0);
        let json = serde_json::to_value(&v.written[0]).unwrap();
        assert!(json["offset_quarters"]["den"].is_number());
    }

    #[test]
    fn out_of_range_time_is_an_error() {
        let ev = EngineEvent::NoteReceived {
            note: 60,
            at_ns: i64::MAX,
        };
        assert!(LiveEvent::from_engine(ev).is_err());
    }
}
