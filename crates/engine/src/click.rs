//! クリック（境界の約束 B0 の 3-3。1a-5）。
//!
//! - 予定時刻の列（[`ClickPlan`]）：前カウントは1小節分、分母の音符ごとに分子の数（Q-29）。曲の間も同じ間隔。
//! - 鳴らす入口（[`AudioOut`]）：実機の [`CpalOut`] と、鳴らさない [`FakeOut`]（試験と流し込み）。
//! - コールバックごとに、cpal のコールバックと再生の時刻の組を記録し、鳴った時刻を `EngineClock` の軸に載せる（B02）。

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use etudition_core::expand::Expansion;
use etudition_core::meter::Meter;
use etudition_core::quarters::Quarters;
use etudition_core::timing::{Timing, position_to_ns};

use crate::clock::EngineClock;
use crate::input::StartError;

/// 予定の1つのクリック。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedClick {
    /// 何番目か（0 始まり。前カウントの最初が 0）。
    pub index: u32,
    /// 予定時刻（ns。`EngineClock` の軸）。
    pub scheduled_ns: i64,
    /// 前カウントか。
    pub count_in: bool,
    /// 強さ：2 は小節の頭、1 は音楽の拍の頭、0 はそのほか（練習で拍を取りやすくするため）。
    pub level: u8,
}

/// クリックの予定時刻の列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickPlan {
    /// 予定時刻の順。
    pub clicks: Vec<PlannedClick>,
}

/// `p` が `unit` の整数倍か。
fn is_multiple(p: Quarters, unit: Quarters) -> bool {
    let n = i128::from(p.num()) * i128::from(unit.den());
    let d = i128::from(unit.num()) * i128::from(p.den());
    d != 0 && n.rem_euclid(d) == 0
}

impl ClickPlan {
    /// 前カウント（位置 −1小節 から）と、曲の間（最後の音の終わりの前まで）のクリック。
    pub fn new(timing: &Timing, meter: Meter, expansion: &Expansion) -> ClickPlan {
        let unit = meter.click_unit();
        let n = i64::from(meter.clicks());
        let measure = meter.measure();
        let beat = meter.beat();
        let end = expansion
            .sounding
            .iter()
            .map(|s| s.start + s.duration)
            .max()
            .unwrap_or(Quarters::ZERO);
        let mut clicks = Vec::new();
        let mut k = -n;
        loop {
            let p = Quarters::new(unit.num() * k, unit.den());
            if k >= 0 && p >= end {
                break;
            }
            let level = if is_multiple(p, measure) {
                2
            } else if is_multiple(p, beat) {
                1
            } else {
                0
            };
            clicks.push(PlannedClick {
                index: u32::try_from(k + n).unwrap_or(u32::MAX),
                scheduled_ns: position_to_ns(timing, p),
                count_in: k < 0,
                level,
            });
            k += 1;
        }
        ClickPlan { clicks }
    }
}

/// cpal の時刻（ns。cpal の軸で、`EngineClock` の軸とは原点が違う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpalTs {
    /// コールバックが呼ばれた時刻。
    pub callback_ns: i64,
    /// そのバッファの最初のフレームが再生される時刻。
    pub playback_ns: i64,
}

/// コールバックごとの組（B02、採点の仕様 v0 の 1-3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickStamp {
    /// 何番目のクリックか。
    pub index: u32,
    /// 予定時刻（ns）。
    pub scheduled_ns: i64,
    /// そのクリックを入れたコールバックの cpal の時刻。
    pub cpal_ts: CpalTs,
    /// 鳴った時刻（ns。`EngineClock` の軸。[`placed_at`] で求めた）。
    pub at_ns: i64,
}

/// 音声から練習のスレッドへ送るもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMsg {
    /// クリックを入れた。
    Stamp(ClickStamp),
    /// 出力先を失った（REQ-107）。
    Lost,
}

/// コールバックを呼んだときの `EngineClock` の時刻と cpal の時刻の組から、バッファの `frame` 番目が鳴る時刻を求める
/// （コールバックの時刻 ＋ 再生までの遅れ ＋ フレームの位置）。遅れが負なら 0 とみなす。
pub fn placed_at(engine_ns_at_callback: i64, ts: CpalTs, frame: u64, sample_rate: u32) -> i64 {
    let latency = ts.playback_ns.saturating_sub(ts.callback_ns).max(0);
    let offset = i128::from(frame) * 1_000_000_000 / i128::from(sample_rate.max(1));
    engine_ns_at_callback
        .saturating_add(latency)
        .saturating_add(i64::try_from(offset).unwrap_or(i64::MAX))
}

/// クリックの音の入口（試験用と実機で同じ）。
pub trait AudioOut: Send {
    /// 鳴らし始める。`sink` には、クリックを入れるたびに `Stamp`、出力先を失ったら `Lost` を送る。
    fn start(
        &mut self,
        clock: EngineClock,
        plan: &ClickPlan,
        sink: rtrb::Producer<AudioMsg>,
    ) -> Result<(), StartError>;
    /// 止める。
    fn stop(&mut self);
}

/// 鳴らさない出力（試験と流し込み。AC-15、AC-46）。始めたときに、決めた cpal の時刻で全部の組を送る。
#[derive(Debug, Clone)]
pub struct FakeOut {
    cpal_base_ns: i64,
    latency_ns: i64,
    missing: bool,
    lost_after: Option<usize>,
}

impl FakeOut {
    /// cpal の時刻は `EngineClock` の時刻 ＋ `cpal_base_ns`。コールバックは予定の `latency_ns` 前に呼ばれ、予定ちょうどに鳴る。
    pub fn new(cpal_base_ns: i64, latency_ns: i64) -> FakeOut {
        FakeOut {
            cpal_base_ns,
            latency_ns,
            missing: false,
            lost_after: None,
        }
    }

    /// 出力先がない（始めると `NoAudioOutput`）。
    pub fn missing() -> FakeOut {
        FakeOut {
            missing: true,
            ..FakeOut::new(0, 0)
        }
    }

    /// `n` 個のクリックの後に出力先を失う。
    pub fn lost_after(self, n: usize) -> FakeOut {
        FakeOut {
            lost_after: Some(n),
            ..self
        }
    }
}

impl AudioOut for FakeOut {
    fn start(
        &mut self,
        _clock: EngineClock,
        plan: &ClickPlan,
        mut sink: rtrb::Producer<AudioMsg>,
    ) -> Result<(), StartError> {
        if self.missing {
            return Err(StartError::NoAudioOutput);
        }
        let n = self.lost_after.unwrap_or(plan.clicks.len());
        for c in plan.clicks.iter().take(n) {
            let engine_cb = c.scheduled_ns - self.latency_ns;
            let cpal_ts = CpalTs {
                callback_ns: self.cpal_base_ns + engine_cb,
                playback_ns: self.cpal_base_ns + engine_cb + self.latency_ns,
            };
            let _ = sink.push(AudioMsg::Stamp(ClickStamp {
                index: c.index,
                scheduled_ns: c.scheduled_ns,
                cpal_ts,
                at_ns: placed_at(engine_cb, cpal_ts, 0, 48_000),
            }));
        }
        if self.lost_after.is_some() {
            let _ = sink.push(AudioMsg::Lost);
        }
        Ok(())
    }

    fn stop(&mut self) {}
}

/// 1つのクリックの長さ（秒）。
const CLICK_SECONDS: f32 = 0.03;
/// コールバックから組を受けて送り先へ渡す間隔。
const RELAY_TICK: Duration = Duration::from_millis(5);

/// cpal で鳴らす出力（実機）。ストリームは自分のスレッドが持つ（`Stream` が `Send` でない裏側があるため）。
pub struct CpalOut {
    volume: f32,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CpalOut {
    /// `volume` は 0.0〜1.0（0 なら音を出さずに時刻だけ記録する。確かめ用）。
    pub fn new(volume: f32) -> CpalOut {
        CpalOut {
            volume: volume.clamp(0.0, 1.0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }
}

/// クリックの音の1つのサンプル（強さで高さと大きさを変え、短く減衰させる）。
fn click_sample(level: u8, t: f32) -> f32 {
    let (freq, amp) = match level {
        2 => (1600.0, 0.9),
        1 => (1100.0, 0.6),
        _ => (800.0, 0.4),
    };
    amp * (TAU * freq * t).sin() * (-t / 0.006).exp()
}

/// コールバックの中の状態（確保も待ちもしない）。
struct Voice {
    clicks: Vec<PlannedClick>,
    next: usize,
    /// 鳴っているクリックの強さと、そのクリックの何フレーム目か。
    playing: Option<(u8, u64)>,
    click_frames: u64,
}

impl Voice {
    /// 1回のコールバック。`frames` フレーム分の音を `write(frame, value)` で書き、入れたクリックの組を `stamp` に渡す。
    fn fill(
        &mut self,
        frames: u64,
        engine_ns: i64,
        ts: CpalTs,
        sample_rate: u32,
        mut write: impl FnMut(u64, f32),
        mut stamp: impl FnMut(ClickStamp),
    ) {
        for f in 0..frames {
            let t = placed_at(engine_ns, ts, f, sample_rate);
            // 予定時刻を過ぎたクリックは、いまのフレームで鳴らし始める（遅れて入れたものも捨てない）
            while let Some(c) = self.clicks.get(self.next) {
                if c.scheduled_ns > t {
                    break;
                }
                stamp(ClickStamp {
                    index: c.index,
                    scheduled_ns: c.scheduled_ns,
                    cpal_ts: ts,
                    at_ns: t,
                });
                self.playing = Some((c.level, 0));
                self.next += 1;
            }
            let v = match self.playing {
                Some((level, i)) if i < self.click_frames => {
                    self.playing = Some((level, i + 1));
                    #[allow(clippy::cast_precision_loss)]
                    click_sample(level, i as f32 / sample_rate as f32)
                }
                _ => {
                    self.playing = None;
                    0.0
                }
            };
            write(f, v);
        }
    }
}

fn stream_ns(i: cpal::StreamInstant) -> i64 {
    i64::try_from(i.as_nanos()).unwrap_or(i64::MAX)
}

/// 型 `T` のサンプルでストリームを作る。
fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    clock: EngineClock,
    mut voice: Voice,
    volume: f32,
    mut stamps: rtrb::Producer<ClickStamp>,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    device.build_output_stream(
        *config,
        move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
            let engine_ns = clock.ns_of(Instant::now());
            let t = info.timestamp();
            let ts = CpalTs {
                callback_ns: stream_ns(t.callback),
                playback_ns: stream_ns(t.playback),
            };
            let frames = (data.len() / channels) as u64;
            voice.fill(
                frames,
                engine_ns,
                ts,
                sample_rate,
                |f, v| {
                    let s = T::from_sample(v * volume);
                    let at = usize::try_from(f).unwrap_or(0) * channels;
                    for x in &mut data[at..at + channels] {
                        *x = s;
                    }
                },
                |s| {
                    let _ = stamps.push(s);
                },
            );
        },
        move |_e| lost.store(true, Ordering::Relaxed),
        None,
    )
}

impl AudioOut for CpalOut {
    fn start(
        &mut self,
        clock: EngineClock,
        plan: &ClickPlan,
        mut sink: rtrb::Producer<AudioMsg>,
    ) -> Result<(), StartError> {
        let clicks = plan.clicks.clone();
        let volume = self.volume;
        let stop = Arc::clone(&self.stop);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), StartError>>();
        self.worker = Some(std::thread::spawn(move || {
            let host = cpal::default_host();
            let Some(device) = host.default_output_device() else {
                let _ = ready_tx.send(Err(StartError::NoAudioOutput));
                return;
            };
            let supported = match device.default_output_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready_tx.send(Err(StartError::Other(format!(
                        "音声の出力の形式を読めない: {e}"
                    ))));
                    return;
                }
            };
            let config = supported.config();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let click_frames = (CLICK_SECONDS * config.sample_rate as f32) as u64;
            let voice = Voice {
                clicks,
                next: 0,
                playing: None,
                click_frames,
            };
            let lost = Arc::new(AtomicBool::new(false));
            let (stamp_tx, mut stamp_rx) = rtrb::RingBuffer::new(1024);
            let built = match supported.sample_format() {
                cpal::SampleFormat::F32 => build::<f32>(
                    &device,
                    &config,
                    clock,
                    voice,
                    volume,
                    stamp_tx,
                    Arc::clone(&lost),
                ),
                cpal::SampleFormat::I16 => build::<i16>(
                    &device,
                    &config,
                    clock,
                    voice,
                    volume,
                    stamp_tx,
                    Arc::clone(&lost),
                ),
                other => {
                    let _ = ready_tx.send(Err(StartError::Other(format!(
                        "音声の形式 {other} には対応していない"
                    ))));
                    return;
                }
            };
            let stream = match built.and_then(|s| s.play().map(|()| s)) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(StartError::Other(format!(
                        "音声の出力を始められない: {e}"
                    ))));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            // コールバックからの組を送り先へ渡し、出力先を失ったら `Lost` を1回送る
            let mut lost_sent = false;
            while !stop.load(Ordering::Relaxed) {
                while let Ok(s) = stamp_rx.pop() {
                    let _ = sink.push(AudioMsg::Stamp(s));
                }
                if !lost_sent && lost.load(Ordering::Relaxed) {
                    let _ = sink.push(AudioMsg::Lost);
                    lost_sent = true;
                }
                std::thread::sleep(RELAY_TICK);
            }
            drop(stream);
        }));
        ready_rx
            .recv()
            .unwrap_or(Err(StartError::Other("音声のスレッドが止まった".into())))
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for CpalOut {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(clicks: &[(i64, u8)]) -> Voice {
        Voice {
            clicks: clicks
                .iter()
                .enumerate()
                .map(|(i, &(at, level))| PlannedClick {
                    index: u32::try_from(i).unwrap(),
                    scheduled_ns: at,
                    count_in: false,
                    level,
                })
                .collect(),
            next: 0,
            playing: None,
            click_frames: 3,
        }
    }

    /// 実機のコールバックの中の置き方：予定時刻のフレームで鳴らし始め、そのフレームの時刻を組にする。遅れたものは先頭で鳴らす。
    #[test]
    fn voice_places_clicks_at_their_frames() {
        // 1 kHz（1フレーム ＝ 1 ms）、遅れ 0。バッファの先頭は時計の 10 ms
        let ts = CpalTs {
            callback_ns: 0,
            playback_ns: 0,
        };
        let mut v = voice(&[(5_000_000, 2), (12_000_000, 0), (13_000_000, 1)]);
        let mut out = [0.0f32; 8];
        let mut stamps = Vec::new();
        v.fill(
            8,
            10_000_000,
            ts,
            1000,
            |f, x| out[usize::try_from(f).unwrap()] = x,
            |s| stamps.push(s),
        );
        // 5 ms の予定は遅れていたので先頭（10 ms）で、12 ms と 13 ms はそれぞれのフレームで
        let at: Vec<i64> = stamps.iter().map(|s| s.at_ns).collect();
        assert_eq!(at, vec![10_000_000, 12_000_000, 13_000_000]);
        // 音は鳴り始めたフレームから click_frames（3）だけ。後のクリックが前のものを切って始める
        // （0 フレーム目は sin(0) で 0。13 ms のクリックは 3〜5 フレーム目、6 フレーム目からは無音）
        assert!(out[1] != 0.0);
        assert!(out[4] != 0.0);
        assert!(out[6..].iter().all(|&x| x == 0.0));
        // 次のコールバックでは、もう入れたクリックを入れない
        stamps.clear();
        v.fill(8, 18_000_000, ts, 1000, |_, _| {}, |s| stamps.push(s));
        assert!(stamps.is_empty());
    }
}
