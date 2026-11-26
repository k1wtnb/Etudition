//! 1回の練習（境界の約束 B0 の 3-4 と4章）。
//!
//! `open_session` はコマンドのスレッドで呼び、機器を1回だけ開く。`OpenSession::run` は練習のスレッドで呼ぶ。
//! 採点は `core` の純粋な関数を、その時刻までの入力で呼び直して決める（状態を持つ逐次の判定器は作らない）。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use etudition_core::expand::Expansion;
use etudition_core::perf::{PerfEvent, PerfKind};
use etudition_core::rules::Rules;
use etudition_core::scoring::{
    NoteMark, RunEnd, RunInput, RunResult, decide_end, marks_closed_between, score_run,
};
use etudition_core::timing::{Timing, position_to_ns, span_to_ns};

use crate::clock::EngineClock;
use crate::input::{InputMsg, InputSink, MidiInput, RawMidi, StartError};
use crate::record::{EventRecord, RecordStatus, RunRecord, TempoRecord, write_run};

/// 期限を待つ間に起きる間隔（`engine` の定数で、採点の規則ではない）。
pub const ENGINE_TICK: Duration = Duration::from_millis(10);
/// 入力から練習のスレッドへの列の長さ。
const QUEUE_CAPACITY: usize = 4096;

/// 回の設定。
pub struct PracticeSetup {
    /// お手本（`check_playable` を通ったもの）。
    pub expansion: Expansion,
    /// お手本のハッシュ（`record::score_hash`）。
    pub score_hash: String,
    /// 練習のテンポと、楽譜の位置 0 の時刻。
    pub timing: Timing,
    /// 前カウントの拍の数（1a-5 で使う）。
    pub count_in_beats: u32,
    /// 補正の値（ns。遅れを正で持つ。1a は 0）。
    pub correction_ns: i64,
    /// この回の記録先のフォルダ（まだないこと）。
    pub record_dir: PathBuf,
    /// 採点規則の版。
    pub rules: &'static Rules,
}

/// 回の進め方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// 時計で進む（実機）。
    Live,
    /// `Finished` まで受けてから決める（流し込み）。時計は読まない。
    Feed,
    /// `Feed` と同じ採点と記録で、通知を時刻どおりに送る（開発用の画面の流し込み。1a-7）。
    /// 受けたノートオンと印は `EngineClock` がその時刻になってから送り、終わりの時刻まで待ってから採点する。
    FeedPaced,
}

/// その回を無効にする理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReason {
    /// MIDI の接続が切れた（REQ-103）。
    Disconnected,
    /// 受信のあふれ（REQ-104。1a-8）。
    Overflow {
        /// あふれた数。
        count: u32,
    },
    /// 音声の出力先がなくなった（REQ-107。1a-5）。
    AudioLost,
}

impl InvalidReason {
    fn text(self) -> String {
        match self {
            InvalidReason::Disconnected => "MIDI の接続が切れた".into(),
            InvalidReason::Overflow { count } => format!("受信のあふれ（{count} 件）"),
            InvalidReason::AudioLost => "音声の出力先がなくなった".into(),
        }
    }
}

/// 画面への通知のもと（殻が `LiveEvent` に直す）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// 回が始まった。カーソルはこのテンポと位置 0 の時刻で進める。
    Started {
        /// テンポと位置 0 の時刻。
        timing: Timing,
    },
    /// クリック（1a-5）。
    Click {
        /// 何番目か。
        index: u32,
        /// 前カウントか。
        count_in: bool,
        /// 予定時刻（ns）。
        scheduled_ns: i64,
    },
    /// ノートオンを受けた。
    NoteReceived {
        /// ノート番号。
        note: u8,
        /// 補正後の時刻（ns）。
        at_ns: i64,
    },
    /// 演奏中の仮の印。
    Marks(Vec<NoteMark>),
    /// 回が終わった。
    Ended {
        /// 採点の結果。
        result: RunResult,
        /// 記録の状態。
        record: RecordStatus,
    },
    /// 回を無効にした。
    Invalid {
        /// 理由。
        reason: InvalidReason,
        /// 記録の状態。
        record: RecordStatus,
    },
}

/// 回の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    /// 採点した（止まった回を含む）。
    Scored(RunResult),
    /// 無効にした。
    Invalid(InvalidReason),
    /// 使う人が閉じた（記録しない。REQ-102）。
    Cancelled,
    /// 始められなかった。
    NotStarted(StartError),
}

/// 機器を開いた回。練習のスレッドへ渡して `run` を呼ぶ。
pub struct OpenSession {
    setup: PracticeSetup,
    input: Box<dyn MidiInput + Send>,
    consumer: rtrb::Consumer<InputMsg>,
    overflow: Arc<AtomicU32>,
    clock: EngineClock,
}

/// 時計と列を作り、入力を開く（コマンドのスレッドで呼ぶ。機器を開くのはここの1回だけ）。
/// 1a-5 で音声の引数を足す。
pub fn open_session(
    setup: PracticeSetup,
    mut input: Box<dyn MidiInput + Send>,
) -> Result<OpenSession, StartError> {
    let clock = EngineClock::new(Instant::now());
    let (producer, consumer) = rtrb::RingBuffer::new(QUEUE_CAPACITY);
    let overflow = Arc::new(AtomicU32::new(0));
    input.open(
        clock,
        InputSink {
            producer,
            overflow: Arc::clone(&overflow),
        },
    )?;
    Ok(OpenSession {
        setup,
        input,
        consumer,
        overflow,
        clock,
    })
}

/// 受けた MIDI のメッセージを、補正後の時刻の `PerfEvent` にする（ノートオンとノートオフだけ）。
fn to_perf(raw: &[RawMidi], correction_ns: i64) -> Vec<PerfEvent> {
    raw.iter()
        .filter_map(|r| {
            let kind = match r.bytes[0] & 0xf0 {
                0x90 => PerfKind::On,
                0x80 => PerfKind::Off,
                _ => return None,
            };
            (r.len >= 3).then_some(PerfEvent {
                t_ns: r.at_ns - correction_ns,
                kind,
                note: r.bytes[1] & 0x7f,
                velocity: r.bytes[2] & 0x7f,
            })
        })
        .collect()
}

impl OpenSession {
    fn input<'a>(&'a self, perf: &'a [PerfEvent]) -> RunInput<'a> {
        RunInput {
            reference: &self.setup.expansion.sounding,
            timing: self.setup.timing,
            events: perf,
            rules: self.setup.rules,
        }
    }

    /// 回を進め、終わったら採点して記録する（4章の手順）。
    pub fn run(
        mut self,
        mode: RunMode,
        notify: &dyn Fn(EngineEvent),
        stop: &AtomicBool,
    ) -> SessionOutcome {
        notify(EngineEvent::Started {
            timing: self.setup.timing,
        });
        let outcome = match mode {
            RunMode::Feed => self.run_feed(false, notify, stop),
            RunMode::FeedPaced => self.run_feed(true, notify, stop),
            RunMode::Live => self.run_live(notify, stop),
        };
        self.input.close();
        outcome
    }

    /// `EngineClock` が `at_ns` になるまで待つ。途中で終了の要求が来たら `false`。
    fn wait_until(&self, at_ns: i64, stop: &AtomicBool) -> bool {
        loop {
            if stop.load(Ordering::Relaxed) {
                return false;
            }
            let left = at_ns.saturating_sub(self.clock.ns_of(Instant::now()));
            if left <= 0 {
                return true;
            }
            let left = Duration::from_nanos(u64::try_from(left).unwrap_or(u64::MAX));
            std::thread::sleep(left.min(ENGINE_TICK));
        }
    }

    fn run_feed(
        &mut self,
        paced: bool,
        notify: &dyn Fn(EngineEvent),
        stop: &AtomicBool,
    ) -> SessionOutcome {
        let mut raw = Vec::new();
        loop {
            if stop.load(Ordering::Relaxed) {
                return SessionOutcome::Cancelled;
            }
            match self.consumer.pop() {
                Ok(InputMsg::Midi(r)) => raw.push(r),
                Ok(InputMsg::Finished) => break,
                Ok(InputMsg::Disconnected) => {
                    return self.invalid(InvalidReason::Disconnected, &raw, notify, stop);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        let perf = to_perf(&raw, self.setup.correction_ns);
        let Some(end) = decide_end(&self.input(&perf), i64::MAX) else {
            return SessionOutcome::NotStarted(StartError::Other("お手本の音がない".into()));
        };
        // 終わりの時刻まで、受けたノートオンと窓が閉じた音の印を、時刻の順に知らせる
        let mut timeline: Vec<(i64, u8, EngineEvent)> = perf
            .iter()
            .filter(|e| e.kind == PerfKind::On && e.velocity != 0 && e.t_ns <= end.at_ns)
            .map(|e| {
                (
                    e.t_ns,
                    0,
                    EngineEvent::NoteReceived {
                        note: e.note,
                        at_ns: e.t_ns,
                    },
                )
            })
            .collect();
        let w = span_to_ns(&self.setup.timing.tempo, self.setup.rules.match_window);
        let mut closes: Vec<i64> = self
            .setup
            .expansion
            .sounding
            .iter()
            .map(|r| position_to_ns(&self.setup.timing, r.start).saturating_add(w))
            .filter(|&c| c <= end.at_ns)
            .collect();
        closes.dedup();
        let mut prev = i64::MIN;
        for c in closes {
            let marks = marks_closed_between(&self.input(&perf), prev, c);
            if !marks.is_empty() {
                timeline.push((c, 1, EngineEvent::Marks(marks)));
            }
            prev = c;
        }
        timeline.sort_by_key(|(t, order, _)| (*t, *order));
        for (t, _, ev) in timeline {
            if paced && !self.wait_until(t, stop) {
                return SessionOutcome::Cancelled;
            }
            notify(ev);
        }
        if paced && !self.wait_until(end.at_ns, stop) {
            return SessionOutcome::Cancelled;
        }
        // 記録の前に、終了の要求をもう一度見る（途中で閉じた回は記録しない。AC-42 の (3)）
        if stop.load(Ordering::Relaxed) {
            return SessionOutcome::Cancelled;
        }
        self.finish(end, &raw, notify)
    }

    fn run_live(&mut self, notify: &dyn Fn(EngineEvent), stop: &AtomicBool) -> SessionOutcome {
        let mut raw: Vec<RawMidi> = Vec::new();
        let mut marked_until = i64::MIN;
        loop {
            if stop.load(Ordering::Relaxed) {
                return SessionOutcome::Cancelled;
            }
            while let Ok(m) = self.consumer.pop() {
                match m {
                    InputMsg::Midi(r) => {
                        if r.bytes[0] & 0xf0 == 0x90 && r.bytes[2] != 0 {
                            notify(EngineEvent::NoteReceived {
                                note: r.bytes[1] & 0x7f,
                                at_ns: r.at_ns - self.setup.correction_ns,
                            });
                        }
                        raw.push(r);
                    }
                    InputMsg::Disconnected => {
                        return self.invalid(InvalidReason::Disconnected, &raw, notify, stop);
                    }
                    InputMsg::Finished => {}
                }
            }
            let now = self.clock.ns_of(Instant::now());
            let perf = to_perf(&raw, self.setup.correction_ns);
            let end = decide_end(&self.input(&perf), now);
            // 終わった回は、終わりの時刻までに窓が閉じた音の印だけを出す
            let until = end.map_or(now, |e| e.at_ns);
            if until > marked_until {
                let marks = marks_closed_between(&self.input(&perf), marked_until, until);
                if !marks.is_empty() {
                    notify(EngineEvent::Marks(marks));
                }
                marked_until = until;
            }
            if let Some(end) = end {
                if stop.load(Ordering::Relaxed) {
                    return SessionOutcome::Cancelled;
                }
                return self.finish(end, &raw, notify);
            }
            std::thread::sleep(ENGINE_TICK);
        }
    }

    /// `result.json` の中身を作る（回の終わりより後に受けた入力は入れない）。
    fn record(
        &self,
        raw: &[RawMidi],
        until_ns: i64,
        result: Option<crate::record::ResultRecord>,
        invalid: Option<InvalidReason>,
    ) -> RunRecord {
        let c = self.setup.correction_ns;
        RunRecord {
            rules_version: self.setup.rules.version,
            score_sha256: self.setup.score_hash.clone(),
            tempo: TempoRecord {
                num: self.setup.timing.tempo.num(),
                den: self.setup.timing.tempo.den(),
            },
            count_in_end_ns: self.setup.timing.origin_ns,
            correction_ns: c,
            events: raw
                .iter()
                .filter(|r| r.at_ns - c <= until_ns)
                .map(|r| EventRecord {
                    raw_ns: r.at_ns,
                    backend_us: r.backend_us,
                    corrected_ns: r.at_ns - c,
                    bytes: r.bytes[..usize::from(r.len.min(3))].to_vec(),
                })
                .collect(),
            clicks: Vec::new(),
            result,
            invalid_reason: invalid.map(InvalidReason::text),
            overflow_count: self.overflow.load(Ordering::Relaxed),
        }
    }

    /// 終わった回を採点し、仕様書 10-1 の表に従って記録する。
    fn finish(&self, end: RunEnd, raw: &[RawMidi], notify: &dyn Fn(EngineEvent)) -> SessionOutcome {
        let perf = to_perf(raw, self.setup.correction_ns);
        let result = score_run(&self.input(&perf), end);
        // 止まった回も2つの記録を書く（Q-51。人間の回答 2026-11-26）。止まった回の結果は点数と合否を持たない
        let kept: Vec<PerfEvent> = perf.into_iter().filter(|e| e.t_ns <= end.at_ns).collect();
        let rec = self.record(raw, end.at_ns, Some((&result).into()), None);
        let record = status(write_run(&self.setup.record_dir, Some(&rec), Some(&kept)));
        notify(EngineEvent::Ended {
            result: result.clone(),
            record,
        });
        SessionOutcome::Scored(result)
    }

    /// 無効の回：採点せず、`result.json` に理由を書き、`performance.mid` も書く（10-1、Q-51。人間の回答 2026-11-26）。
    fn invalid(
        &self,
        reason: InvalidReason,
        raw: &[RawMidi],
        notify: &dyn Fn(EngineEvent),
        stop: &AtomicBool,
    ) -> SessionOutcome {
        // 記録の前に、すでに届いた終了の要求を見る（閉じた回は記録しない。REQ-102）
        if stop.load(Ordering::Relaxed) {
            return SessionOutcome::Cancelled;
        }
        let rec = self.record(raw, i64::MAX, None, Some(reason));
        let perf = to_perf(raw, self.setup.correction_ns);
        let record = status(write_run(&self.setup.record_dir, Some(&rec), Some(&perf)));
        notify(EngineEvent::Invalid { reason, record });
        SessionOutcome::Invalid(reason)
    }
}

fn status(r: Result<crate::record::RunFiles, crate::record::RecordError>) -> RecordStatus {
    match r {
        Ok(files) => RecordStatus::Written(files),
        Err(e) => RecordStatus::Failed(e.0),
    }
}
