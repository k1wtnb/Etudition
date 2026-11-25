//! 1a-1：採点に使える楽譜かの確かめ（AC-03、AC-45）と、日本語のパスの読み込み（AC-04）。

use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::score::read_score;

/// 読み込み → 展開 → 確かめ。止まったら理由の文字列（要素の名前を含む）を返す。
fn load_playable(bytes: &[u8]) -> Result<Expansion, Vec<String>> {
    let loaded = read_score(bytes).map_err(|e| vec![format!("読めない: {e}")])?;
    let e = expand(&loaded.score);
    check_playable(&e).map_err(|issues| {
        issues
            .iter()
            .map(|i| format!("{:?} {}: {}", i.measure_index, i.element, i.detail))
            .collect::<Vec<String>>()
    })?;
    Ok(e)
}

/// 1小節の楽譜を作る。`notes` は measure の中身。
fn one_measure(notes: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<score-partwise><part-list/><part id="P1"><measure number="1">
<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
{notes}
</measure></part></score-partwise>"#
    )
}

const C4: &str =
    "<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration></note>";

#[test]
fn s1_fixtures_are_playable() {
    for bytes in [
        &include_bytes!("../../../fixtures/s1/01-tie.musicxml")[..],
        include_bytes!("../../../fixtures/s1/01-tie.mxl"),
        include_bytes!("../../../fixtures/s1/02-chord.musicxml"),
        include_bytes!("../../../fixtures/s1/03-two-voices.musicxml"),
        include_bytes!("../../../fixtures/s1/04-grand-staff.musicxml"),
        include_bytes!("../../../fixtures/s1/05-repeat-volta.musicxml"),
        include_bytes!("../../../fixtures/s1/06-ds-coda.musicxml"),
        include_bytes!("../../../fixtures/s1/07-pickup.musicxml"),
        include_bytes!("../../../fixtures/s1/08-grace.musicxml"),
    ] {
        assert!(load_playable(bytes).is_ok());
    }
}

/// AC-05：装飾音が、お手本の音の列に書かれた音として入る。
#[test]
fn grace_is_in_the_reference() {
    let e = load_playable(include_bytes!("../../../fixtures/s1/08-grace.musicxml"))
        .expect("使えるはず");
    let grace: Vec<_> = e.sounding.iter().filter(|s| s.is_grace).collect();
    assert_eq!(grace.len(), 1);
    assert_eq!(grace[0].pitch, 74);
}

/// AC-45 の1：壊れた XML。
#[test]
fn broken_xml_stops() {
    let why = load_playable(b"<score-partwise><part id=\"P1\">").expect_err("止まるはず");
    assert!(why[0].contains("XML として読めない"), "{why:?}");
}

/// AC-45 の2：音符が0個（休符だけ）。
#[test]
fn no_notes_stops() {
    let xml = one_measure("<note><rest/><duration>4</duration></note>");
    let why = load_playable(xml.as_bytes()).expect_err("止まるはず");
    assert_eq!(why, ["None note: 音高のある音符が0個（採点できない）"]);
}

/// AC-45 の3：パートが2つ以上。
#[test]
fn two_parts_stop() {
    let xml = one_measure(C4).replace(
        "</part></score-partwise>",
        "</part><part id=\"P2\"/></score-partwise>",
    );
    let why = load_playable(xml.as_bytes()).expect_err("止まるはず");
    assert!(why.iter().any(|w| w.starts_with("None part:")), "{why:?}");
}

/// AC-03、AC-45 の4：対応していない要素。名前と位置（小節）の一覧が空でなく返る。
#[test]
fn unsupported_element_stops_with_its_name_and_place() {
    let xml = one_measure(&format!(
        "{C4}<direction><direction-type><words>Fine</words></direction-type><sound fine=\"yes\"/></direction>"
    ));
    let why = load_playable(xml.as_bytes()).expect_err("止まるはず");
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(why[0].starts_with("Some(0) sound@fine:"), "{why:?}");
}

/// AC-45 の5：終わらない繰り返し（展開の歩数の上限を超える）。
#[test]
fn endless_repeat_stops() {
    let xml = one_measure(&format!(
        "{C4}<barline location=\"right\"><repeat direction=\"backward\" times=\"1000\"/></barline>"
    ));
    let why = load_playable(xml.as_bytes()).expect_err("止まるはず");
    assert!(
        why.iter()
            .any(|w| w.contains("repeat: 繰り返しの展開が終わらない")),
        "{why:?}"
    );
}

/// AC-04：日本語を含むパスにある `.mxl` と `.musicxml` を読める。
#[test]
fn reads_files_under_a_japanese_path() {
    let dir =
        std::env::temp_dir().join(format!("etudition-日本語のフォルダ-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("フォルダを作れるはず");
    let files = [
        (
            "練習曲.musicxml",
            &include_bytes!("../../../fixtures/s1/01-tie.musicxml")[..],
        ),
        (
            "練習曲.mxl",
            include_bytes!("../../../fixtures/s1/01-tie.mxl"),
        ),
    ];
    for (name, bytes) in files {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("書けるはず");
        let read = std::fs::read(&path).expect("日本語のパスから読めるはず");
        let e = load_playable(&read).expect("使えるはず");
        assert_eq!(e.sounding.len(), 3, "{name}");
    }
    std::fs::remove_dir_all(&dir).expect("消せるはず");
}
