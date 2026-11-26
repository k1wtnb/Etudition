//! 誤り入りの MIDI を作る（計画 1a-3、REQ-069）。
//!
//! お手本（展開済みの鳴る音）から、楽譜どおりの MIDI（弱起の音から始まる形）を作り、誤りを入れる。
//! 誤りの種類と期待する減点は [`CASES`]（採点の仕様 v0 の 9-2 の表の行に対応）。期待値は合成の楽譜 T1
//! （`fixtures/1a/t1.musicxml`）のもので、道具も T1 だけを受け付ける（任意の曲の期待値は計算しない）。

use etudition_core::expand::Expansion;
use etudition_core::quarters::Quarters;

/// 道具が受け付ける楽譜（T1）。
pub const T1: &[u8] = include_bytes!("../../../fixtures/1a/t1.musicxml");

/// MIDI の四分音符あたりの tick。
pub const PPQ: i64 = 960;

/// 入れる誤り。`index` はお手本（`Expansion::sounding`）の何番目か。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 音高を `delta` 半音ずらす。
    Pitch {
        /// お手本の何番目か。
        index: usize,
        /// ずらす半音の数。
        delta: i32,
    },
    /// 弾かない。
    Drop {
        /// お手本の何番目か。
        index: usize,
    },
    /// 音を足す（位置と長さは四分音符の数）。
    Add {
        /// ノート番号。
        pitch: u8,
        /// 位置（分子、分母）。
        start: (i64, i64),
        /// 長さ（分子、分母）。
        len: (i64, i64),
    },
    /// ノートオンとノートオフを `by` だけずらす（`index` が `None` なら全部）。
    Shift {
        /// お手本の何番目か。
        index: Option<usize>,
        /// ずらす量（分子、分母）。
        by: (i64, i64),
    },
    /// 長さを変える。
    Length {
        /// お手本の何番目か。
        index: usize,
        /// 新しい長さ（分子、分母）。
        len: (i64, i64),
    },
    /// ノートオフを送らない。
    NoOff {
        /// お手本の何番目か。
        index: usize,
    },
}

/// 誤りの種類と、期待する誤りの数（音程、タイミング、長さ、組の数）。
#[derive(Debug, Clone, Copy)]
pub struct ErrorCase {
    /// ファイルの名前に使う名前。
    pub name: &'static str,
    /// 採点の仕様 v0 の試験表の行。
    pub row: &'static str,
    /// 入れる誤り。
    pub errors: &'static [ErrorKind],
    /// 期待する（音程、タイミング、長さ、組）の数。
    pub expected: (u32, u32, u32, usize),
}

const fn q(num: i64, den: i64) -> (i64, i64) {
    (num, den)
}

/// T1（`fixtures/1a/t1.musicxml`）に入れる誤りの表。お手本の並びは SR-01（和音は C4、E4、G4 の順）。
pub const CASES: &[ErrorCase] = &[
    ErrorCase {
        name: "as-written",
        row: "ST-01",
        errors: &[],
        expected: (0, 0, 0, 10),
    },
    ErrorCase {
        name: "wrong-pitch",
        row: "ST-02",
        errors: &[ErrorKind::Pitch { index: 1, delta: 1 }],
        expected: (1, 0, 0, 10),
    },
    ErrorCase {
        name: "octave",
        row: "ST-03",
        errors: &[ErrorKind::Pitch {
            index: 0,
            delta: 12,
        }],
        expected: (1, 0, 0, 10),
    },
    ErrorCase {
        name: "missing",
        row: "ST-04",
        errors: &[ErrorKind::Drop { index: 1 }],
        expected: (1, 0, 0, 9),
    },
    ErrorCase {
        name: "extra",
        row: "ST-05",
        errors: &[ErrorKind::Add {
            pitch: 69,
            start: q(1, 4),
            len: q(1, 4),
        }],
        expected: (1, 0, 0, 10),
    },
    ErrorCase {
        name: "late",
        row: "ST-06",
        errors: &[ErrorKind::Shift {
            index: Some(1),
            by: q(1, 4),
        }],
        expected: (0, 1, 0, 10),
    },
    ErrorCase {
        name: "late-at-tolerance",
        row: "ST-07",
        errors: &[ErrorKind::Shift {
            index: Some(1),
            by: q(1, 8),
        }],
        expected: (0, 0, 0, 10),
    },
    ErrorCase {
        name: "outside-window",
        row: "ST-08",
        errors: &[ErrorKind::Shift {
            index: Some(1),
            by: q(3, 4),
        }],
        expected: (2, 0, 0, 9),
    },
    ErrorCase {
        name: "too-short",
        row: "ST-09",
        errors: &[ErrorKind::Length {
            index: 1,
            len: q(2, 5),
        }],
        expected: (0, 0, 1, 10),
    },
    ErrorCase {
        name: "length-at-half",
        row: "ST-09",
        errors: &[ErrorKind::Length {
            index: 1,
            len: q(1, 2),
        }],
        expected: (0, 0, 0, 10),
    },
    ErrorCase {
        name: "length-at-one-and-half",
        row: "ST-09",
        errors: &[ErrorKind::Length {
            index: 1,
            len: q(3, 2),
        }],
        expected: (0, 0, 0, 10),
    },
    ErrorCase {
        name: "too-long",
        row: "ST-09",
        errors: &[ErrorKind::Length {
            index: 1,
            len: q(8, 5),
        }],
        expected: (0, 0, 1, 10),
    },
    ErrorCase {
        name: "missing-chord-note",
        row: "ST-12",
        errors: &[ErrorKind::Drop { index: 5 }],
        expected: (1, 0, 0, 9),
    },
    ErrorCase {
        name: "missing-repeated-note",
        row: "ST-14",
        errors: &[ErrorKind::Drop { index: 8 }],
        expected: (1, 0, 0, 9),
    },
    ErrorCase {
        name: "all-late",
        row: "ST-18",
        errors: &[ErrorKind::Shift {
            index: None,
            by: q(1, 4),
        }],
        expected: (0, 10, 0, 10),
    },
    ErrorCase {
        name: "wrong-pitch-and-late",
        row: "ST-19",
        errors: &[
            ErrorKind::Pitch { index: 1, delta: 1 },
            ErrorKind::Shift {
                index: Some(1),
                by: q(1, 4),
            },
        ],
        expected: (1, 1, 0, 10),
    },
    ErrorCase {
        name: "no-note-off",
        row: "ST-20",
        errors: &[ErrorKind::NoOff { index: 9 }],
        expected: (0, 0, 1, 10),
    },
];

fn frac((num, den): (i64, i64)) -> Quarters {
    Quarters::new(num, den)
}

/// お手本の位置を tick にする（弱起の音から始まる形：1つ目の小節の頭が tick 0）。
fn tick(e: &Expansion, position: Quarters) -> i64 {
    let origin = e
        .measures
        .first()
        .map_or(Quarters::ZERO, |m| m.start.min(Quarters::ZERO));
    let p = position - origin;
    p.num() * PPQ / p.den()
}

/// お手本から、誤りを入れた MIDI（形式 0、四分音符 = `PPQ` tick）を作る。
pub fn make_error_midi(e: &Expansion, errors: &[ErrorKind]) -> Vec<u8> {
    use midly::num::{u4, u7, u15, u28};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    };
    // (ノート番号, 始まり, 長さ, ノートオフを送るか)
    let mut notes: Vec<Option<(i32, Quarters, Quarters, bool)>> = e
        .sounding
        .iter()
        .map(|s| Some((s.pitch, s.start, s.duration, true)))
        .collect();
    let mut added = Vec::new();
    for err in errors {
        match *err {
            ErrorKind::Pitch { index, delta } => {
                if let Some(Some(n)) = notes.get_mut(index) {
                    n.0 += delta;
                }
            }
            ErrorKind::Drop { index } => {
                if let Some(n) = notes.get_mut(index) {
                    *n = None;
                }
            }
            ErrorKind::Add { pitch, start, len } => {
                added.push((i32::from(pitch), frac(start), frac(len), true));
            }
            ErrorKind::Shift { index, by } => {
                for (i, n) in notes.iter_mut().enumerate() {
                    if (index.is_none() || index == Some(i))
                        && let Some(n) = n
                    {
                        n.1 = n.1 + frac(by);
                    }
                }
            }
            ErrorKind::Length { index, len } => {
                if let Some(Some(n)) = notes.get_mut(index) {
                    n.2 = frac(len);
                }
            }
            ErrorKind::NoOff { index } => {
                if let Some(Some(n)) = notes.get_mut(index) {
                    n.3 = false;
                }
            }
        }
    }
    // (tick, ノートオフが先, ノート番号, オンか)
    let mut evs: Vec<(i64, u8, u8, bool)> = Vec::new();
    for (pitch, start, len, off) in notes.into_iter().flatten().chain(added) {
        let key = pitch.clamp(0, 127) as u8;
        evs.push((tick(e, start), 1, key, true));
        if off {
            evs.push((tick(e, start + len), 0, key, false));
        }
    }
    evs.sort_unstable();
    let mut track = Vec::new();
    let mut prev = 0;
    for (t, _, key, on) in evs {
        let message = if on {
            MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(100),
            }
        } else {
            MidiMessage::NoteOff {
                key: u7::new(key),
                vel: u7::new(0),
            }
        };
        track.push(TrackEvent {
            delta: u28::new(u32::try_from((t - prev).max(0)).unwrap_or(0)),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message,
            },
        });
        prev = t;
    }
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    let smf = Smf {
        header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(PPQ as u16))),
        tracks: vec![track],
    };
    let mut out = Vec::new();
    // Vec への書き込みは失敗しない
    let _ = smf.write_std(&mut out);
    out
}
