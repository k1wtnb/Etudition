//! MIDI の入力（境界の約束 B0 の 3-2）。流し込み（`FileInput`）と実機（1a-8 の `MidirInput`）は同じ入口を通る。

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use etudition_core::expand::Expansion;
use etudition_core::quarters::Quarters;
use etudition_core::timing::{Timing, position_to_ns};

use crate::clock::EngineClock;

/// コールバックで作る1つの MIDI のメッセージ。生の時刻を捨てない（B02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMidi {
    /// `EngineClock` の時刻（ns。補正前）。
    pub at_ns: i64,
    /// MIDI の裏側の時刻（µs。midir の時刻。流し込みは `None`）。
    pub backend_us: Option<u64>,
    /// メッセージのバイト列（先頭 `len` バイトが有効）。
    pub bytes: [u8; 3],
    /// 有効なバイト数。
    pub len: u8,
}

/// 入力から練習のスレッドへ送るもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMsg {
    /// MIDI のメッセージ。
    Midi(RawMidi),
    /// 接続が切れた（REQ-103）。
    Disconnected,
    /// 流し込みの送り終わり（`FileInput` だけが送る）。
    Finished,
}

/// 入力の送り先。列と、あふれの数。
pub struct InputSink {
    /// 練習のスレッドへの列。
    pub producer: rtrb::Producer<InputMsg>,
    /// 入力を捨てた数（REQ-104）。捨てたときだけ足す。
    pub overflow: Arc<AtomicU32>,
}

/// 始められない理由（REQ-103、REQ-107）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// MIDI 機器がない。
    NoMidiDevice,
    /// 音声の出力先がない。
    NoAudioOutput,
    /// そのほか。
    Other(String),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::NoMidiDevice => f.write_str("MIDI 機器がない"),
            StartError::NoAudioOutput => f.write_str("音声の出力先がない"),
            StartError::Other(s) => f.write_str(s),
        }
    }
}

/// 入力の入口（境界の約束 B0 の 3-2）。
pub trait MidiInput: Send {
    /// 入力を開き、`clock` で時刻を付けて `sink` に送り始める。
    fn open(&mut self, clock: EngineClock, sink: InputSink) -> Result<(), StartError>;
    /// 入力を閉じる。
    fn close(&mut self);
}

/// MIDI ファイルを読めない理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedError(pub String);

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 楽譜どおりの MIDI の流し込み（計画 2-3 の1。試験と開発用だけ。REQ-106）。
///
/// 時刻はファイルのテンポではなく、楽譜の位置から作る：位置 ＝ tick ÷ 四分音符あたりの tick − 原点のずれ。
/// 原点のずれは楽譜の弱起の長さ（MIDI は弱起の音から始まる形とする。計画の案）。
pub struct FileInput {
    msgs: Vec<InputMsg>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FileInput {
    /// MIDI ファイルのバイト列を読み、送るメッセージの列を作る。
    pub fn new(
        midi: &[u8],
        expansion: &Expansion,
        timing: &Timing,
    ) -> Result<FileInput, FeedError> {
        let smf = midly::Smf::parse(midi)
            .map_err(|e| FeedError(format!("MIDI ファイルとして読めない: {e}")))?;
        let ppq = match smf.header.timing {
            midly::Timing::Metrical(t) => i64::from(t.as_int()),
            midly::Timing::Timecode(..) => {
                return Err(FeedError(
                    "時間の単位が SMPTE の MIDI ファイルは読まない".into(),
                ));
            }
        };
        if ppq == 0 {
            return Err(FeedError("四分音符あたりの tick が 0".into()));
        }
        // 弱起の長さ（1つ目の小節の頭が負の位置なら、その長さ）
        let pickup = expansion
            .measures
            .first()
            .map(|m| m.start)
            .filter(|s| *s < Quarters::ZERO)
            .map_or(Quarters::ZERO, |s| Quarters::ZERO - s);
        // (tick, トラック, トラックの中の順番, バイト列)
        let mut notes: Vec<(i64, usize, usize, [u8; 3])> = Vec::new();
        for (ti, track) in smf.tracks.iter().enumerate() {
            let mut tick: i64 = 0;
            for (k, ev) in track.iter().enumerate() {
                tick += i64::from(ev.delta.as_int());
                if let midly::TrackEventKind::Midi { channel, message } = ev.kind {
                    let ch = channel.as_int();
                    let bytes = match message {
                        midly::MidiMessage::NoteOn { key, vel } => {
                            [0x90 | ch, key.as_int(), vel.as_int()]
                        }
                        midly::MidiMessage::NoteOff { key, vel } => {
                            [0x80 | ch, key.as_int(), vel.as_int()]
                        }
                        _ => continue,
                    };
                    notes.push((tick, ti, k, bytes));
                }
            }
        }
        notes.sort_by_key(|&(tick, ti, k, _)| (tick, ti, k));
        let msgs = notes
            .into_iter()
            .map(|(tick, _, _, bytes)| {
                let position = Quarters::new(tick, ppq) - pickup;
                InputMsg::Midi(RawMidi {
                    at_ns: position_to_ns(timing, position),
                    backend_us: None,
                    bytes,
                    len: 3,
                })
            })
            .collect();
        Ok(FileInput {
            msgs,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        })
    }

    /// 試験用：時刻 `at_ns` 以降のメッセージの代わりに、接続が切れたことを送る（1a-9、REQ-103）。
    pub fn disconnect_at(mut self, at_ns: i64) -> FileInput {
        let cut = self
            .msgs
            .iter()
            .position(|m| matches!(m, InputMsg::Midi(r) if r.at_ns >= at_ns))
            .unwrap_or(self.msgs.len());
        self.msgs.truncate(cut);
        self.msgs.push(InputMsg::Disconnected);
        self
    }
}

impl MidiInput for FileInput {
    /// 自分のスレッドで送り、送り終わりを待たずに戻る。列が満杯なら、捨てずに空くまで待つ（待ったことはあふれに数えない）。
    fn open(&mut self, _clock: EngineClock, sink: InputSink) -> Result<(), StartError> {
        let mut msgs = std::mem::take(&mut self.msgs);
        if !matches!(msgs.last(), Some(InputMsg::Disconnected)) {
            msgs.push(InputMsg::Finished);
        }
        let stop = Arc::clone(&self.stop);
        let mut producer = sink.producer;
        self.worker = Some(std::thread::spawn(move || {
            for m in msgs {
                let mut m = m;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    match producer.push(m) {
                        Ok(()) => break,
                        Err(rtrb::PushError::Full(back)) => {
                            m = back;
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                }
            }
        }));
        Ok(())
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for FileInput {
    fn drop(&mut self) {
        self.close();
    }
}
