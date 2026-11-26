//! F0 のダミー機器へ、シリアル（ST-LINK の仮想 COM ポート）で送る指示を作る（`midi-send`）。
//!
//! 指示はファームの `App/Inc/dummy_cmd.h` と同じ1行の形（`ping`、`on <note> <vel>`、`off <note>`）。
//! ファームは `ok`、`busy`、`pong`、`err` のどれかを1行で返す。

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// `midi-send` の指示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendCmd {
    /// つながっているかを確かめる。
    Ping,
    /// Note On（チャンネル 1）。
    On { note: u8, vel: u8 },
    /// Note Off（チャンネル 1）。
    Off { note: u8 },
    /// ノート 60 を `count` 回、`interval_ms` ごとに押し、間隔の半分で離す（S2b の揺れの記録用）。
    Beat { count: u32, interval_ms: u64 },
}

/// 拍の間隔の下限（押して離すまでを、返事を待ちながら収めるため）。
pub const BEAT_INTERVAL_MIN_MS: u64 = 20;

/// ボタンと同じ音（ファームの `DUMMY_APP_BUTTON_NOTE` などと合わせる）。
pub const BEAT_NOTE: u8 = 60;
pub const BEAT_VELOCITY: u8 = 100;

fn number<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse::<T>()
        .map_err(|_| format!("{what} が数として読めない: {s}"))
}

fn midi_value(s: &str, what: &str, min: u8) -> Result<u8, String> {
    let v: u8 = number(s, what)?;
    if v < min || v > 127 {
        return Err(format!("{what} は {min}〜127: {s}"));
    }
    Ok(v)
}

/// ポート名の後ろの引数を指示として読む。
pub fn parse(args: &[String]) -> Result<SendCmd, String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["ping"] => Ok(SendCmd::Ping),
        ["on", note, vel] => Ok(SendCmd::On {
            note: midi_value(note, "ノート番号", 0)?,
            vel: midi_value(vel, "ベロシティ", 1)?,
        }),
        ["off", note] => Ok(SendCmd::Off {
            note: midi_value(note, "ノート番号", 0)?,
        }),
        ["beat", count, interval] => {
            let count: u32 = number(count, "回数")?;
            let interval_ms: u64 = number(interval, "間隔（ms）")?;
            if count == 0 {
                return Err("回数は 1 以上".into());
            }
            if interval_ms < BEAT_INTERVAL_MIN_MS {
                return Err(format!("間隔は {BEAT_INTERVAL_MIN_MS} ms 以上"));
            }
            Ok(SendCmd::Beat { count, interval_ms })
        }
        _ => Err(
            "指示は ping | on <ノート> <ベロシティ> | off <ノート> | beat <回数> <間隔ms>".into(),
        ),
    }
}

/// 送る行と、送る時刻（始めからの ms）の列。
pub fn schedule(cmd: &SendCmd) -> Vec<(u64, String)> {
    match *cmd {
        SendCmd::Ping => vec![(0, "ping\n".into())],
        SendCmd::On { note, vel } => vec![(0, format!("on {note} {vel}\n"))],
        SendCmd::Off { note } => vec![(0, format!("off {note}\n"))],
        SendCmd::Beat { count, interval_ms } => (0..u64::from(count))
            .flat_map(|k| {
                let at = k * interval_ms;
                [
                    (at, format!("on {BEAT_NOTE} {BEAT_VELOCITY}\n")),
                    (at + interval_ms / 2, format!("off {BEAT_NOTE}\n")),
                ]
            })
            .collect(),
    }
}

/// 送った行に対して、ファームの返事が成功を表すか。
pub fn reply_ok(sent: &str, reply: &str) -> bool {
    if sent.starts_with("ping") {
        reply == "pong"
    } else {
        reply == "ok"
    }
}

/// `COM5` のような名前を、Windows で開ける形（`\\.\COM5`）にする。ほかの名前はそのまま。
pub fn port_path(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with("COM") && upper[3..].chars().all(|c| c.is_ascii_digit()) && upper.len() > 3
    {
        format!(r"\\.\{upper}")
    } else {
        name.to_string()
    }
}

/// 返事を1行待つ。来なければ誤り。
pub fn wait_reply(rx: &Receiver<String>, timeout: Duration) -> Result<String, String> {
    match rx.recv_timeout(timeout) {
        Ok(line) => Ok(line),
        Err(RecvTimeoutError::Timeout) => Err(format!(
            "返事が {} ms 来ない（ポート、配線、ファームを確かめる）",
            timeout.as_millis()
        )),
        Err(RecvTimeoutError::Disconnected) => Err("ポートが閉じた".into()),
    }
}

/// 受けたバイトを行に分ける。`\r` は捨てる。終わっていない行は `partial` に残す。
pub fn split_lines(partial: &mut String, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for &b in bytes {
        match b {
            b'\r' => {}
            b'\n' => out.push(std::mem::take(partial)),
            _ => partial.push(char::from(b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parse_reads_each_command_and_rejects_out_of_range_values() {
        assert_eq!(parse(&args("ping")), Ok(SendCmd::Ping));
        assert_eq!(
            parse(&args("on 60 100")),
            Ok(SendCmd::On { note: 60, vel: 100 })
        );
        assert_eq!(parse(&args("off 127")), Ok(SendCmd::Off { note: 127 }));
        assert_eq!(
            parse(&args("beat 100 250")),
            Ok(SendCmd::Beat {
                count: 100,
                interval_ms: 250
            })
        );
        for bad in [
            "",
            "pong",
            "on 60",
            "on 60 0",
            "on 128 1",
            "off -1",
            "beat 0 250",
            "beat 10 19",
            "on 60 100 1",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn schedule_matches_the_firmware_lines_and_beat_releases_at_half_interval() {
        assert_eq!(schedule(&SendCmd::Ping), vec![(0, "ping\n".to_string())]);
        assert_eq!(
            schedule(&SendCmd::On { note: 60, vel: 100 }),
            vec![(0, "on 60 100\n".to_string())]
        );
        assert_eq!(
            schedule(&SendCmd::Beat {
                count: 2,
                interval_ms: 250
            }),
            vec![
                (0, "on 60 100\n".to_string()),
                (125, "off 60\n".to_string()),
                (250, "on 60 100\n".to_string()),
                (375, "off 60\n".to_string()),
            ]
        );
    }

    #[test]
    fn replies_and_port_names() {
        assert!(reply_ok("ping\n", "pong"));
        assert!(!reply_ok("ping\n", "ok"));
        assert!(reply_ok("on 60 100\n", "ok"));
        assert!(!reply_ok("on 60 100\n", "busy"));
        assert!(!reply_ok("off 60\n", "err"));
        assert_eq!(port_path("COM5"), r"\\.\COM5");
        assert_eq!(port_path("com12"), r"\\.\COM12");
        assert_eq!(port_path("COM"), "COM");
        assert_eq!(port_path("/dev/ttyACM0"), "/dev/ttyACM0");
    }

    #[test]
    fn lines_are_split_across_reads_and_waiting_times_out() {
        let mut partial = String::new();
        assert_eq!(split_lines(&mut partial, b"po"), Vec::<String>::new());
        assert_eq!(
            split_lines(&mut partial, b"ng\r\nok\nb"),
            vec!["pong", "ok"]
        );
        assert_eq!(partial, "b");

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        assert!(wait_reply(&rx, Duration::from_millis(5)).is_err());
        tx.send("ok".into()).unwrap();
        assert_eq!(wait_reply(&rx, Duration::from_millis(5)), Ok("ok".into()));
    }
}
