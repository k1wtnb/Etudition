//! 演奏の入力（時刻付きの MIDI のイベント）と、そこから演奏の音を作る処理（採点の仕様 v0 の 3-1）。

/// ノートオンかノートオフか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfKind {
    /// ノートオン（ベロシティ 0 はノートオフとして扱う。SR-07）。
    On,
    /// ノートオフ。
    Off,
}

/// 補正後の時刻の1イベント（境界の約束 B0 の 2-3）。並びは受け取った順。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfEvent {
    /// 補正後の時刻（ns。原点は前カウントの最初のクリック）。
    pub t_ns: i64,
    /// ノートオンかノートオフか。
    pub kind: PerfKind,
    /// ノート番号。
    pub note: u8,
    /// ベロシティ。
    pub velocity: u8,
}

/// 演奏の1音。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct PlayedNote {
    /// ノート番号。
    pub note: u8,
    /// ノートオンの時刻（ns）。
    pub on_ns: i64,
    /// ノートオフの時刻（ns）。回の終わりまでに離されなければ、回の終わりの時刻。
    pub off_ns: i64,
}

/// イベントの列から演奏の音を作る（SR-07、SR-08）。
///
/// - 時刻の順、同じ時刻ではノートオフを先、さらに同じなら受け取った順に並べる。
/// - ノート番号ごとに、ノートオンで開き、次のノートオフで閉じる。開いている音と同じ番号のノートオンは、前の音をその時刻で閉じる。
/// - 開いていない音のノートオフは捨てる。
/// - 時刻が `end_ns` より後のイベントは捨てる。`end_ns` に離されていない音は `end_ns` で閉じる（SR-09 の後半、SR-21）。
///
/// 戻り値の並びはノートオンの時刻の順（同じなら受け取った順）。組の2つ目は、ノートオンを受け取った順番
/// （`events` の添字。和音の中で同じ音高を受け取った順に並べるため。SR-10）。
pub(crate) fn build_notes(events: &[PerfEvent], end_ns: i64) -> Vec<(PlayedNote, usize)> {
    let is_off = |e: &PerfEvent| e.kind == PerfKind::Off || e.velocity == 0;
    let mut order: Vec<usize> = (0..events.len()).collect();
    // ノートオフを先にする（false < true なので、ノートオンを true にする）
    order.sort_by_key(|&i| (events[i].t_ns, !is_off(&events[i]), i));
    // (ノートオンの時刻、受け取った順) と、閉じた音
    let mut done: Vec<((i64, usize), PlayedNote)> = Vec::new();
    let mut open: [Option<(i64, usize)>; 128] = [None; 128];
    for i in order {
        let e = &events[i];
        if e.t_ns > end_ns {
            continue;
        }
        let slot = &mut open[usize::from(e.note & 0x7f)];
        if let Some((on, seq)) = slot.take() {
            done.push((
                (on, seq),
                PlayedNote {
                    note: e.note,
                    on_ns: on,
                    off_ns: e.t_ns,
                },
            ));
        }
        if !is_off(e) {
            *slot = Some((e.t_ns, i));
        }
    }
    for (note, slot) in open.iter().enumerate() {
        if let Some((on, seq)) = *slot {
            done.push((
                (on, seq),
                PlayedNote {
                    note: note as u8,
                    on_ns: on,
                    off_ns: end_ns,
                },
            ));
        }
    }
    done.sort_by_key(|(key, _)| *key);
    done.into_iter().map(|((_, seq), n)| (n, seq)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t_ns: i64, kind: PerfKind, note: u8, velocity: u8) -> PerfEvent {
        PerfEvent {
            t_ns,
            kind,
            note,
            velocity,
        }
    }
    use PerfKind::{Off, On};

    fn notes(events: &[PerfEvent], end: i64) -> Vec<(u8, i64, i64)> {
        build_notes(events, end)
            .iter()
            .map(|(n, _)| (n.note, n.on_ns, n.off_ns))
            .collect()
    }

    /// ベロシティ 0 のノートオンはノートオフ（SR-07）。
    #[test]
    fn velocity_zero_is_note_off() {
        let e = [ev(0, On, 60, 100), ev(10, On, 60, 0)];
        assert_eq!(notes(&e, 100), [(60, 0, 10)]);
    }

    /// 同じ時刻のノートオフとノートオンは、ノートオフを先にする（SR-07）。受け取った順が逆でも同じ。
    #[test]
    fn off_before_on_at_same_time() {
        let e = [
            ev(0, On, 60, 100),
            ev(10, On, 60, 100),
            ev(10, Off, 60, 0),
            ev(20, Off, 60, 0),
        ];
        assert_eq!(notes(&e, 100), [(60, 0, 10), (60, 10, 20)]);
    }

    /// 開いている音をもう一度押したら、前の音をその時刻で閉じる（SR-08）。
    #[test]
    fn repress_closes_previous() {
        let e = [ev(0, On, 60, 100), ev(5, On, 60, 100), ev(9, Off, 60, 0)];
        assert_eq!(notes(&e, 100), [(60, 0, 5), (60, 5, 9)]);
    }

    /// 相手のないノートオフは捨てる（SR-08）。
    #[test]
    fn stray_note_off_is_dropped() {
        let e = [ev(0, Off, 61, 0), ev(1, On, 60, 100), ev(2, Off, 60, 0)];
        assert_eq!(notes(&e, 100), [(60, 1, 2)]);
    }

    /// 回の終わりより後のイベントは捨て、離されていない音は終わりの時刻で閉じる。終わりちょうどのノートオンは入る。
    #[test]
    fn events_after_end_are_dropped() {
        let e = [
            ev(0, On, 60, 100),
            ev(50, On, 62, 100),
            ev(60, On, 64, 100),
            ev(70, Off, 60, 0),
        ];
        assert_eq!(notes(&e, 50), [(60, 0, 50), (62, 50, 50)]);
    }
}
