//! 演奏ごとの記録（`performance.mid` と `result.json`。境界の約束 B0 の 3-5、仕様書10章、REQ-102）。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use etudition_core::perf::{PerfEvent, PerfKind};
use etudition_core::scoring::{Items, RunEnd, RunResult};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// お手本のハッシュ：楽譜のファイルのバイト列の SHA-256（16 進の小文字。BQ-02）。
pub fn score_hash(score_file: &[u8]) -> String {
    Sha256::digest(score_file)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 練習のテンポ（四分音符／分 ＝ `num / den`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TempoRecord {
    /// 分子。
    pub num: u32,
    /// 分母。
    pub den: u32,
}

/// 受けた MIDI のメッセージ1つ。補正前と補正後の時刻を両方持つ（B02、REQ-087）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRecord {
    /// 生の時刻（`EngineClock` の ns。補正前）。
    pub raw_ns: i64,
    /// MIDI の裏側の時刻（µs）。
    pub backend_us: Option<u64>,
    /// 補正後の時刻（ns）＝ 生の時刻 − 補正の値（採点の仕様 v0 の SR-02）。
    pub corrected_ns: i64,
    /// メッセージのバイト列。
    pub bytes: Vec<u8>,
}

/// クリック1つの記録（1a-5 で中身を足す）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClickRecord {
    /// 何番目のクリックか（0 始まり）。
    pub index: u32,
    /// 予定時刻（ns）。
    pub scheduled_ns: i64,
}

/// `result.json` に保存する採点の結果（点数、合否、回の終わり）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResultRecord {
    /// 採点規則の版。
    pub rules_version: &'static str,
    /// 回の終わり。
    pub end: RunEnd,
    /// 3項目の点数（止まった回は `None`）。
    pub items: Option<Items>,
    /// 合否（止まった回は `None`）。
    pub passed: Option<bool>,
}

impl From<&RunResult> for ResultRecord {
    fn from(r: &RunResult) -> ResultRecord {
        ResultRecord {
            rules_version: r.rules_version,
            end: r.end,
            items: r.items,
            passed: r.passed,
        }
    }
}

/// `result.json` の中身（仕様書10章の「`result.json` に入れるもの」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunRecord {
    /// 採点規則の版。
    pub rules_version: &'static str,
    /// お手本のハッシュ（[`score_hash`]）。
    pub score_sha256: String,
    /// 練習のテンポ。
    pub tempo: TempoRecord,
    /// 前カウントの終わり（楽譜の位置 0）の時刻（ns）。
    pub count_in_end_ns: i64,
    /// 補正の値（ns。遅れを正で持つ）。
    pub correction_ns: i64,
    /// 受けた MIDI のメッセージ（回の終わりまで）。
    pub events: Vec<EventRecord>,
    /// クリック（1a-5）。
    pub clicks: Vec<ClickRecord>,
    /// 採点の結果のうち、決まっている項目（無効の回は `None`）。音ごとの判定は保存しない（仕様書10章では提案）。
    pub result: Option<ResultRecord>,
    /// 無効の理由（REQ-103、REQ-104、REQ-107）。
    pub invalid_reason: Option<String>,
    /// 受信のあふれの数（REQ-104）。
    pub overflow_count: u32,
}

/// 書いたファイル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFiles {
    /// `result.json`。
    pub json: Option<PathBuf>,
    /// `performance.mid`。
    pub midi: Option<PathBuf>,
}

/// 記録の書き込みの状態（画面の「記録できなかった」の知らせのもと）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordStatus {
    /// 書いた。
    Written(RunFiles),
    /// 書けなかった（書きかけは消した）。
    Failed(String),
    /// 書かない回（使う人が閉じた回、仕様書 10-1 で未確定の欄）。
    NotWritten,
}

/// 記録を書けなかった理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordError(pub String);

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 1回の演奏の記録を、フォルダ `dir` に書く（`dir` はまだないこと。書いたら変えない）。
///
/// `result.json` と `performance.mid` は別々に選べる。まず `dir` の隣の一時フォルダに書き、書き終えてから名前を変える。
/// 失敗したら一時フォルダを消し、書きかけのファイルを残さない（REQ-102、AC-42）。
pub fn write_run(
    dir: &Path,
    json: Option<&RunRecord>,
    midi: Option<&[PerfEvent]>,
) -> Result<RunFiles, RecordError> {
    write_run_inner(dir, json, midi, false)
}

fn write_run_inner(
    dir: &Path,
    json: Option<&RunRecord>,
    midi: Option<&[PerfEvent]>,
    fail_before_rename: bool,
) -> Result<RunFiles, RecordError> {
    let err = |what: &str, e: &dyn fmt::Display| RecordError(format!("{what}: {e}"));
    if dir.exists() {
        return Err(RecordError(format!(
            "記録先がすでにある: {}",
            dir.display()
        )));
    }
    let name = dir
        .file_name()
        .ok_or_else(|| RecordError(format!("記録先の名前がない: {}", dir.display())))?;
    let mut partial_name = std::ffi::OsString::from(".");
    partial_name.push(name);
    partial_name.push(".partial");
    let partial = dir.with_file_name(partial_name);
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).map_err(|e| err("記録先のフォルダを作れない", &e))?;
    }
    // 一時フォルダがすでにあれば（同じ記録先へほかが書いている、または前の書きかけ）、消さずに失敗にする。
    // 消してよいのは、ここで作った一時フォルダだけ
    fs::create_dir(&partial)
        .map_err(|e| err("一時フォルダを作れない（すでにあるか、書けない）", &e))?;
    let written = (|| -> Result<(), RecordError> {
        if let Some(r) = json {
            let text = serde_json::to_string_pretty(r).map_err(|e| err("JSON にできない", &e))?;
            fs::write(partial.join("result.json"), text)
                .map_err(|e| err("result.json を書けない", &e))?;
        }
        if let Some(events) = midi {
            fs::write(partial.join("performance.mid"), midi_bytes(events)?)
                .map_err(|e| err("performance.mid を書けない", &e))?;
        }
        if fail_before_rename {
            return Err(RecordError("試験用の失敗".into()));
        }
        fs::rename(&partial, dir).map_err(|e| err("名前を変えられない", &e))
    })();
    if let Err(e) = written {
        let _ = fs::remove_dir_all(&partial);
        return Err(e);
    }
    Ok(RunFiles {
        json: json.map(|_| dir.join("result.json")),
        midi: midi.map(|_| dir.join("performance.mid")),
    })
}

/// `performance.mid` の四分音符あたりの tick と、テンポ（µs／四分音符。120 BPM）。1 tick は約 0.52 ms。
/// 正確な時刻は `result.json` にある（聞き直しと確かめ用のファイル）。
const MIDI_PPQ: u16 = 960;
const MIDI_TEMPO_US: u32 = 500_000;

fn midi_bytes(events: &[PerfEvent]) -> Result<Vec<u8>, RecordError> {
    use midly::num::{u4, u7, u15, u24, u28};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    };
    let to_tick = |ns: i64| -> i64 {
        let ns = i128::from(ns.max(0));
        let t = ns * i128::from(MIDI_PPQ) / (i128::from(MIDI_TEMPO_US) * 1000);
        i64::try_from(t).unwrap_or(i64::MAX)
    };
    let mut sorted: Vec<&PerfEvent> = events.iter().collect();
    sorted.sort_by_key(|e| e.t_ns);
    let mut track = vec![TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(MIDI_TEMPO_US))),
    }];
    let mut prev = 0_i64;
    for e in sorted {
        let tick = to_tick(e.t_ns);
        let delta = u32::try_from(tick - prev)
            .map_err(|_| RecordError("MIDI の時刻が大きすぎる".into()))?;
        prev = tick;
        let (key, vel) = (u7::new(e.note & 0x7f), u7::new(e.velocity & 0x7f));
        let message = match e.kind {
            PerfKind::On => MidiMessage::NoteOn { key, vel },
            PerfKind::Off => MidiMessage::NoteOff { key, vel },
        };
        track.push(TrackEvent {
            delta: u28::new(delta),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message,
            },
        });
    }
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    let smf = Smf {
        header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(MIDI_PPQ))),
        tracks: vec![track],
    };
    let mut out = Vec::new();
    smf.write_std(&mut out)
        .map_err(|e| RecordError(format!("MIDI にできない: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("etudition-record-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn record() -> RunRecord {
        RunRecord {
            rules_version: "v0",
            score_sha256: score_hash(b"x"),
            tempo: TempoRecord { num: 60, den: 1 },
            count_in_end_ns: 4_000_000_000,
            correction_ns: 0,
            events: Vec::new(),
            clicks: Vec::new(),
            result: None,
            invalid_reason: None,
            overflow_count: 0,
        }
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(
            score_hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// AC-42 の (2)：書き終える前に止まったら、記録先にも一時フォルダにも何も残らない。
    #[test]
    fn failure_before_rename_leaves_nothing() {
        let base = tmp("partial");
        let dir = base.join("run1");
        let ev = [PerfEvent {
            t_ns: 0,
            kind: PerfKind::On,
            note: 60,
            velocity: 100,
        }];
        assert!(write_run_inner(&dir, Some(&record()), Some(&ev), true).is_err());
        assert!(!dir.exists());
        assert_eq!(fs::read_dir(&base).map(|d| d.count()).unwrap_or(0), 0);
        let _ = fs::remove_dir_all(&base);
    }

    /// 書いたら変えない：同じ記録先には2度書かない。
    #[test]
    fn existing_dir_is_not_overwritten() {
        let base = tmp("twice");
        let dir = base.join("run1");
        write_run(&dir, Some(&record()), None).expect("1回目は書けるはず");
        assert!(write_run(&dir, Some(&record()), None).is_err());
        let _ = fs::remove_dir_all(&base);
    }
}
