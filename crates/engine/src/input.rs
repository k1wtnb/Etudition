//! MIDI の入力（境界の約束 B0 の 3-2）。流し込み（`FileInput`）と実機（1a-8 の `MidirInput`）は同じ入口を通る。

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
    /// 閉じずに、呼んだときまでに受けた分（切断の知らせを含む）を送り先へ渡し切るよう頼む（待たない）。
    /// 返した番号を `flushed` に渡す。中継を持たない入力は何もしない。
    fn request_flush(&mut self) -> u64 {
        0
    }
    /// `request_flush` で頼んだ分を送り先へ渡し終えたか。送り先が満杯なら、読まれるまで終わらない。
    fn flushed(&self, _ticket: u64) -> bool {
        true
    }
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

    /// 試験用：送るメッセージの列。
    pub fn messages(&self) -> &[InputMsg] {
        &self.msgs
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

/// midir のコールバックで受けたバイト列を `RawMidi` にする（確保しない）。3バイトより長いもの（SysEx など）と空のものは捨てる。
pub fn raw_midi(at_ns: i64, backend_us: u64, msg: &[u8]) -> Option<RawMidi> {
    if msg.is_empty() || msg.len() > 3 {
        return None;
    }
    let mut bytes = [0u8; 3];
    bytes[..msg.len()].copy_from_slice(msg);
    Some(RawMidi {
        at_ns,
        backend_us: Some(backend_us),
        bytes,
        len: u8::try_from(msg.len()).unwrap_or(3),
    })
}

/// コールバックで受けたメッセージを内側の列に入れる。入らなければ捨てて、あふれに数える（待たず、確保しない。REQ-104）。
pub fn accept(tx: &mut rtrb::Producer<InputMsg>, overflow: &AtomicU32, msg: InputMsg) {
    if tx.push(msg).is_err() {
        overflow.fetch_add(1, Ordering::Relaxed);
    }
}

/// 保留と内側の列を、順番どおりに送り先へ渡す。送り先が満杯なら、渡せなかったものを保留に残す。全部渡せたら真。
pub fn relay_input(
    pending: &mut Option<InputMsg>,
    rx: &mut rtrb::Consumer<InputMsg>,
    out: &mut rtrb::Producer<InputMsg>,
) -> bool {
    while let Some(m) = pending.take().or_else(|| rx.pop().ok()) {
        if let Err(rtrb::PushError::Full(back)) = out.push(m) {
            *pending = Some(back);
            return false;
        }
    }
    true
}

/// 渡せずに残ったメッセージの数。
pub fn left_over(pending: &Option<InputMsg>, rx: &rtrb::Consumer<InputMsg>) -> usize {
    usize::from(pending.is_some()) + rx.slots()
}

/// 入力の機器の名前の一覧から、使う機器を選ぶ。なければ `None`（`NoMidiDevice`）。今は最初のもの（S2b と S3 で見直す）。
pub fn choose_port(names: &[String]) -> Option<usize> {
    (!names.is_empty()).then_some(0)
}

/// コールバックから受けたメッセージの内側の列の長さ。
const MIDIR_QUEUE: usize = 4096;
/// 内側の列から送り先へ渡す間隔。
const MIDIR_RELAY_TICK: Duration = Duration::from_millis(1);
/// 機器が抜けたかを調べる間隔。
const MIDIR_WATCH: Duration = Duration::from_millis(500);

/// 実機の MIDI の入力（midir。裏側は既定のもの。Windows は WinMM。裏側の選び方は S2b で見直す）。
///
/// midir のコールバックは時刻を付けて内側の列に入れるだけ（待たず、確保しない。入らなければあふれに数える）。
/// 自分のスレッドが内側の列から送り先へ渡し、機器が抜けたら `Disconnected` を送る（REQ-103）。
pub struct MidirInput {
    conn: Option<midir::MidiInputConnection<()>>,
    stop: Arc<AtomicBool>,
    /// 渡し切りの要求の番号と、済んだ番号（`flush`）。
    flush: Arc<(AtomicU64, AtomicU64)>,
    worker: Option<JoinHandle<()>>,
    /// 開いた機器の名前。
    pub port_name: Option<String>,
}

impl Default for MidirInput {
    fn default() -> Self {
        MidirInput::new()
    }
}

impl MidirInput {
    /// まだ開いていない入力。
    pub fn new() -> MidirInput {
        MidirInput {
            conn: None,
            stop: Arc::new(AtomicBool::new(false)),
            flush: Arc::new((AtomicU64::new(0), AtomicU64::new(0))),
            worker: None,
            port_name: None,
        }
    }
}

/// `id` の機器がまだあるか。
fn port_present(id: &str) -> bool {
    midir::MidiInput::new("Etudition-watch").is_ok_and(|m| m.find_port_by_id(id).is_some())
}

impl MidiInput for MidirInput {
    fn open(&mut self, clock: EngineClock, sink: InputSink) -> Result<(), StartError> {
        // MIDI の仕組みそのものが使えないときも、機器がないとみなす（REQ-103）
        let mut input = midir::MidiInput::new("Etudition").map_err(|_| StartError::NoMidiDevice)?;
        // SysEx、タイミングクロック、アクティブセンシングは使わない
        input.ignore(midir::Ignore::All);
        let ports = input.ports();
        let names: Vec<String> = ports
            .iter()
            .map(|p| input.port_name(p).unwrap_or_default())
            .collect();
        let Some(i) = choose_port(&names) else {
            return Err(StartError::NoMidiDevice);
        };
        let port = &ports[i];
        let id = port.id();
        let (mut tx, mut rx) = rtrb::RingBuffer::<InputMsg>::new(MIDIR_QUEUE);
        let overflow = Arc::clone(&sink.overflow);
        // 切断を見つけたら受付を締める門。コールバックは入る前に `busy` を上げ、`closed` を見る。
        // 中継は `closed` を立ててから `busy` が 0 になるのを待つ（SeqCst。走っている途中のコールバックの音も渡すため）
        let gate = Arc::new((AtomicBool::new(false), AtomicU32::new(0)));
        let cb_gate = Arc::clone(&gate);
        let cb_overflow = Arc::clone(&overflow);
        let conn = input
            .connect(
                port,
                "etudition-in",
                move |stamp_us, msg, ()| {
                    let (closed, busy) = &*cb_gate;
                    busy.fetch_add(1, Ordering::SeqCst);
                    let at_ns = clock.ns_of(std::time::Instant::now());
                    if let Some(r) = raw_midi(at_ns, stamp_us, msg) {
                        if closed.load(Ordering::SeqCst) {
                            // 切断を知らせた後に届いた音は渡せない。黙って捨てず、あふれに数える
                            cb_overflow.fetch_add(1, Ordering::Relaxed);
                        } else {
                            accept(&mut tx, &cb_overflow, InputMsg::Midi(r));
                        }
                    }
                    busy.fetch_sub(1, Ordering::SeqCst);
                },
                (),
            )
            .map_err(|e| StartError::Other(format!("MIDI 機器を開けない: {e}")))?;
        let stop = Arc::clone(&self.stop);
        let flush = Arc::clone(&self.flush);
        let mut producer = sink.producer;
        self.worker = Some(std::thread::spawn(move || {
            let mut last_watch = std::time::Instant::now();
            let mut pending: Option<InputMsg> = None;
            // 止めるとき：受付はもう締めてある（close が先に機器を閉じる）。残りを渡し、渡せない分はあふれに数える（黙って捨てない）
            let give_up = |pending: &Option<InputMsg>, rx: &rtrb::Consumer<InputMsg>| {
                let n = u32::try_from(left_over(pending, rx)).unwrap_or(u32::MAX);
                overflow.fetch_add(n, Ordering::Relaxed);
            };
            loop {
                // 送り先が満杯なら、捨てずに次の周回で入れ直す（ここはコールバックではないので待ってよい）
                let stopping = stop.load(Ordering::Acquire);
                let asked = flush.0.load(Ordering::SeqCst);
                if asked > flush.1.load(Ordering::Acquire) {
                    // 渡し切りの要求：要求より前に走り始めたコールバックが終わるのを待ち、全部渡してから済みにする
                    while gate.1.load(Ordering::SeqCst) > 0 {
                        std::thread::yield_now();
                    }
                    while !relay_input(&mut pending, &mut rx, &mut producer) {
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        std::thread::sleep(MIDIR_RELAY_TICK);
                    }
                    flush.1.store(asked, Ordering::Release);
                }
                relay_input(&mut pending, &mut rx, &mut producer);
                if stopping {
                    give_up(&pending, &rx);
                    return;
                }
                if last_watch.elapsed() >= MIDIR_WATCH {
                    last_watch = std::time::Instant::now();
                    if !port_present(&id) {
                        // 抜けた：受付を締め、走っているコールバックが終わるのを待ってから、
                        // すでに受けた音を全部渡し、切断を知らせる（Astra の監査）
                        gate.0.store(true, Ordering::SeqCst);
                        while gate.1.load(Ordering::SeqCst) > 0 {
                            std::thread::yield_now();
                        }
                        loop {
                            if relay_input(&mut pending, &mut rx, &mut producer)
                                && producer.push(InputMsg::Disconnected).is_ok()
                            {
                                return;
                            }
                            if stop.load(Ordering::Acquire) {
                                give_up(&pending, &rx);
                                return;
                            }
                            std::thread::sleep(MIDIR_RELAY_TICK);
                        }
                    }
                }
                std::thread::sleep(MIDIR_RELAY_TICK);
            }
        }));
        self.conn = Some(conn);
        self.port_name = names.into_iter().nth(i);
        Ok(())
    }

    fn request_flush(&mut self) -> u64 {
        self.flush.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn flushed(&self, ticket: u64) -> bool {
        // 中継が終わっていれば（切断を知らせた後など）、渡せるものはもう渡してある
        self.flush.1.load(Ordering::Acquire) >= ticket
            || self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    fn close(&mut self) {
        // 先に機器を閉じて受付を締める（もうコールバックは来ない）。それから中継に残りを渡させて止める
        if let Some(c) = self.conn.take() {
            let _ = c.close();
        }
        self.stop.store(true, Ordering::Release);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for MidirInput {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod midir_tests {
    use super::*;

    /// midir のコールバックで受けたバイト列を `RawMidi` にする。3バイトより長いもの（SysEx など）と空のものは捨てる。
    #[test]
    fn raw_midi_keeps_short_messages_with_both_times() {
        let r = raw_midi(1_500, 42, &[0x90, 60, 100]).unwrap();
        assert_eq!(
            r,
            RawMidi {
                at_ns: 1_500,
                backend_us: Some(42),
                bytes: [0x90, 60, 100],
                len: 3,
            }
        );
        let one = raw_midi(7, 1, &[0xfe]).unwrap();
        assert_eq!((one.bytes, one.len), ([0xfe, 0, 0], 1));
        assert!(raw_midi(0, 0, &[]).is_none());
        assert!(raw_midi(0, 0, &[0xf0, 1, 2, 0xf7]).is_none());
    }

    /// 入力の機器がなければ選ばない（`NoMidiDevice`。REQ-103）。あれば最初のものを使う（S2b と S3 で見直す）。
    #[test]
    fn choose_port_takes_the_first_or_none() {
        assert_eq!(choose_port(&[]), None);
        assert_eq!(choose_port(&["F767 MIDI".into(), "other".into()]), Some(0));
    }

    fn note(n: u8) -> InputMsg {
        InputMsg::Midi(raw_midi(i64::from(n), 0, &[0x90, n, 100]).unwrap())
    }

    /// コールバックの投入：内側の列が満杯なら捨てて、あふれに数える（REQ-104、AC-44）。
    #[test]
    fn accept_counts_what_does_not_fit() {
        let (mut tx, mut rx) = rtrb::RingBuffer::new(1);
        let overflow = AtomicU32::new(0);
        accept(&mut tx, &overflow, note(60));
        accept(&mut tx, &overflow, note(62));
        assert_eq!(overflow.load(Ordering::Relaxed), 1);
        assert_eq!(rx.pop(), Ok(note(60)));
    }

    /// 中継：送り先が満杯なら残りを保留し、空いたら順番どおりに渡す（Astra の監査）。
    #[test]
    fn relay_keeps_order_when_the_sink_is_full() {
        let (mut inner_tx, mut inner_rx) = rtrb::RingBuffer::new(8);
        for n in [60, 62, 64] {
            inner_tx.push(note(n)).unwrap();
        }
        let (mut out, mut got) = rtrb::RingBuffer::new(2);
        let mut pending = None;
        assert!(!relay_input(&mut pending, &mut inner_rx, &mut out));
        assert_eq!((got.pop(), got.pop()), (Ok(note(60)), Ok(note(62))));
        assert!(relay_input(&mut pending, &mut inner_rx, &mut out));
        assert_eq!(got.pop(), Ok(note(64)));
        assert_eq!(left_over(&pending, &inner_rx), 0);
    }

    /// 渡せずに残った数（止めるときにあふれに足す）。
    #[test]
    fn left_over_counts_pending_and_queued() {
        let (mut inner_tx, mut inner_rx) = rtrb::RingBuffer::new(8);
        for n in [60, 62] {
            inner_tx.push(note(n)).unwrap();
        }
        let (mut out, _got) = rtrb::RingBuffer::new(0);
        let mut pending = None;
        assert!(!relay_input(&mut pending, &mut inner_rx, &mut out));
        assert_eq!(left_over(&pending, &inner_rx), 2);
    }
}
