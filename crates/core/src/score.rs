//! MusicXML（`.musicxml`、`.xml`、`.mxl`）を読み、書かれた音符と小節の列を作る（S1）。
//!
//! 読むのは、1つ目のパートの音符・休符・`backup`・`forward`・小節線の繰り返し・`sound` の跳び先だけ。
//! 鳴る音に影響しうるのに読まない要素は、黙って捨てずに [`Issue`] の一覧で返す（B07、REQ-105）。
//! 表示だけの要素（符尾、連桁、スラーなど）は [`IGNORED_NOTE_CHILDREN`] などに名前を書いて、わざと読まない。

use std::fmt;

use crate::quarters::Quarters;

/// 書かれた音符の ID。パート・小節の番号（文書の中の順番、0 始まり）・声部・小節の中の順番（声部ごと、0 始まり）。
///
/// 和音の音もそれぞれ1つの ID を持つ。装飾音も数える。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WrittenNoteId {
    /// パートの `id` 属性。
    pub part: String,
    /// 小節の文書の中の順番（0 始まり）。`number` 属性は弱起やかっこで重なりうるので使わない。
    pub measure_index: usize,
    /// `voice` 要素の文字列（ないときは `"1"`）。
    pub voice: String,
    /// 同じ小節・同じ声部の中での順番（0 始まり、文書の順）。
    pub order: usize,
}

/// `P1/m0/v1/n0` の形で書く。
impl fmt::Display for WrittenNoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/m{}/v{}/n{}",
            self.part, self.measure_index, self.voice, self.order
        )
    }
}

/// 楽譜に書かれた1つの音符（休符は含まない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenNote {
    /// 音符の ID。
    pub id: WrittenNoteId,
    /// 小節の `number` 属性（表示用）。
    pub measure_number: String,
    /// 譜表の番号（1 始まり。`staff` 要素がなければ 1）。
    pub staff: u32,
    /// 音高（MIDI のノート番号。中央の C ＝ C4 ＝ 60）。
    pub pitch: i32,
    /// 小節の頭からの位置（四分音符の数）。
    pub offset: Quarters,
    /// 書かれた長さ（四分音符の数）。装飾音は 0。
    pub duration: Quarters,
    /// `<tie type="start"/>` がある。
    pub tie_start: bool,
    /// `<tie type="stop"/>` がある。
    pub tie_stop: bool,
    /// 装飾音（`<grace/>`）。
    pub is_grace: bool,
}

/// 1つの小節（書かれた形。繰り返しを展開する前）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Measure {
    /// 文書の中の順番（0 始まり）。
    pub index: usize,
    /// `number` 属性。
    pub number: String,
    /// 長さ（四分音符の数）。中身の終わりの最大。中身がなければ拍子の長さ。
    pub length: Quarters,
    /// `implicit="yes"`（弱起など、数えない小節）。
    pub implicit: bool,
    /// 左の小節線に前向きの繰り返し記号がある。
    pub repeat_forward: bool,
    /// 右の小節線に後ろ向きの繰り返し記号があるとき、その部分を弾く回数（`times`、既定 2）。
    pub repeat_backward: Option<u32>,
    /// この小節で始まるかっこの番号（`<ending type="start" number="1, 2">`）。
    pub ending_start: Option<Vec<u32>>,
    /// この小節でかっこが終わる（`stop` か `discontinue`）。
    pub ending_end: bool,
    /// `<sound segno="..."/>` の名前。
    pub segno: Option<String>,
    /// `<sound coda="..."/>` の名前（Coda の行き先の小節）。
    pub coda: Option<String>,
    /// `<sound tocoda="..."/>` の名前（D.S. の後、この小節の終わりで Coda へ跳ぶ）。
    pub tocoda: Option<String>,
    /// `<sound dalsegno="..."/>` の名前（この小節の終わりで segno へ戻る）。
    pub dalsegno: Option<String>,
}

impl Default for Quarters {
    fn default() -> Self {
        Quarters::ZERO
    }
}

/// 読まなかった（読めない）要素。名前と場所。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// 小節の文書の中の順番（小節の外なら `None`）。
    pub measure_index: Option<usize>,
    /// 要素や属性の名前（例：`harmony`、`sound@fine`）。
    pub element: String,
    /// 説明。
    pub detail: String,
}

/// 読んだ楽譜（1つ目のパート）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Score {
    /// パートの `id`。
    pub part: String,
    /// 小節（文書の順）。
    pub measures: Vec<Measure>,
    /// 書かれた音符（文書の順）。
    pub notes: Vec<WrittenNote>,
    /// 読まなかった要素の一覧。
    pub issues: Vec<Issue>,
}

/// 読み込みの結果。OSMD に渡す XML の文字列も返す（`.mxl` は展開した後の XML）。
#[derive(Debug, Clone)]
pub struct LoadedScore {
    /// 楽譜の XML の文字列。
    pub xml: String,
    /// 読んだ楽譜。
    pub score: Score,
}

/// 読み込みを続けられない誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError(pub String);

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `measure` の子のうち、表示だけなのでわざと読まないもの。
pub const IGNORED_MEASURE_CHILDREN: &[&str] = &["print"];
/// `note` の子のうち、表示だけなのでわざと読まないもの。
pub const IGNORED_NOTE_CHILDREN: &[&str] = &[
    "type",
    "dot",
    "stem",
    "beam",
    "notations",
    "time-modification",
    "accidental",
    "notehead",
    "instrument",
];
/// `attributes` の子のうち、鳴る音に影響しないのでわざと読まないもの。
pub const IGNORED_ATTRIBUTES_CHILDREN: &[&str] = &["key", "staves", "clef", "staff-details"];
/// `sound` の属性のうち、S1 では使わないもの（テンポは練習のテンポを使う。計画 2-3 の1）。
pub const IGNORED_SOUND_ATTRIBUTES: &[&str] = &["tempo", "dynamics"];

/// MusicXML のバイト列（`.mxl` の zip でもよい）を読む。
pub fn read_score(bytes: &[u8]) -> Result<LoadedScore, ReadError> {
    let xml = if bytes.starts_with(b"PK\x03\x04") {
        unzip_mxl(bytes)?
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|e| ReadError(format!("UTF-8 でない: {e}")))?
    };
    let score = parse_xml(&xml)?;
    Ok(LoadedScore { xml, score })
}

fn unzip_mxl(bytes: &[u8]) -> Result<String, ReadError> {
    use std::io::Read;
    let err = |e: &dyn fmt::Display| ReadError(format!(".mxl を開けない: {e}"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(&e))?;
    let read = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| {
        let mut s = String::new();
        zip.by_name(name)
            .map_err(|e| err(&e))?
            .read_to_string(&mut s)
            .map_err(|e| err(&e))?;
        Ok::<String, ReadError>(s)
    };
    let container = read(&mut zip, "META-INF/container.xml")?;
    let doc = roxmltree::Document::parse(&container).map_err(|e| err(&e))?;
    let root = doc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| ReadError("container.xml に rootfile がない".into()))?
        .to_string();
    read(&mut zip, &root)
}

/// 子の要素の文字列。
fn child_text<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.has_tag_name(name))
        .and_then(|c| c.text())
        .map(str::trim)
}

fn parse_int<T: std::str::FromStr>(s: Option<&str>, what: &str) -> Result<T, ReadError> {
    let s = s.ok_or_else(|| ReadError(format!("{what} がない")))?;
    s.parse()
        .map_err(|_| ReadError(format!("{what} が整数でない: {s}")))
}

fn parse_xml(xml: &str) -> Result<Score, ReadError> {
    // MuseScore の書き出しは DOCTYPE（外の DTD の参照）を持つので許す。外の DTD は読みに行かない。
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, opt)
        .map_err(|e| ReadError(format!("XML として読めない: {e}")))?;
    let root = doc.root_element();
    if !root.has_tag_name("score-partwise") {
        return Err(ReadError(format!(
            "score-partwise ではない: {}",
            root.tag_name().name()
        )));
    }
    let mut parts = root.children().filter(|n| n.has_tag_name("part"));
    let part = parts
        .next()
        .ok_or_else(|| ReadError("part がない".into()))?;
    let part_id = part.attribute("id").unwrap_or("").to_string();
    let mut measures = Vec::new();
    let mut notes = Vec::new();
    let mut issues = Vec::new();
    for extra in parts {
        issues.push(Issue {
            measure_index: None,
            element: "part".into(),
            detail: format!(
                "パートが2つ以上ある。1つ目だけを読んだ（{}）",
                extra.attribute("id").unwrap_or("")
            ),
        });
    }

    let mut divisions: Option<i64> = None;
    let mut time_length = Quarters::new(4, 1);
    for (index, m) in part
        .children()
        .filter(|n| n.has_tag_name("measure"))
        .enumerate()
    {
        let mut measure = Measure {
            index,
            number: m.attribute("number").unwrap_or("").to_string(),
            implicit: m.attribute("implicit") == Some("yes"),
            ..Measure::default()
        };
        let mut issue = |element: &str, detail: String| {
            issues.push(Issue {
                measure_index: Some(index),
                element: element.into(),
                detail,
            })
        };
        let mut cursor = Quarters::ZERO;
        let mut end = Quarters::ZERO;
        let mut last_start = Quarters::ZERO;
        // 声部ごとの次の順番（文書の順に出てくる声部の列。順番の決まる入れ物だけを使う）
        let mut orders: Vec<(String, usize)> = Vec::new();
        for c in m.children().filter(|c| c.is_element()) {
            let name = c.tag_name().name();
            match name {
                "attributes" => {
                    for a in c.children().filter(|a| a.is_element()) {
                        match a.tag_name().name() {
                            "divisions" => {
                                divisions = Some(parse_int(a.text().map(str::trim), "divisions")?)
                            }
                            "time" => {
                                let beats: i64 = parse_int(child_text(a, "beats"), "beats")?;
                                let beat_type: i64 =
                                    parse_int(child_text(a, "beat-type"), "beat-type")?;
                                time_length = Quarters::new(beats * 4, beat_type);
                            }
                            n if IGNORED_ATTRIBUTES_CHILDREN.contains(&n) => {}
                            n => issue(n, "attributes の中の読まない要素".into()),
                        }
                    }
                }
                "backup" | "forward" => {
                    let d =
                        to_quarters(parse_int(child_text(c, "duration"), "duration")?, divisions)?;
                    cursor = if name == "backup" {
                        cursor - d
                    } else {
                        cursor + d
                    };
                    end = end.max(cursor);
                }
                "note" => {
                    let is_chord = c.children().any(|x| x.has_tag_name("chord"));
                    let is_grace = c.children().any(|x| x.has_tag_name("grace"));
                    let is_rest = c.children().any(|x| x.has_tag_name("rest"));
                    let duration = if is_grace {
                        Quarters::ZERO
                    } else {
                        to_quarters(parse_int(child_text(c, "duration"), "duration")?, divisions)?
                    };
                    let start = if is_chord { last_start } else { cursor };
                    for x in c.children().filter(|x| x.is_element()) {
                        let n = x.tag_name().name();
                        let known = matches!(
                            n,
                            "pitch"
                                | "rest"
                                | "duration"
                                | "voice"
                                | "staff"
                                | "chord"
                                | "grace"
                                | "tie"
                        ) || IGNORED_NOTE_CHILDREN.contains(&n);
                        if !known {
                            issue(n, "note の中の読まない要素".into());
                        }
                    }
                    let pitch_node = c.children().find(|x| x.has_tag_name("pitch"));
                    if let (false, Some(p)) = (is_rest, pitch_node) {
                        {
                            let voice = child_text(c, "voice").unwrap_or("1").to_string();
                            let order = match orders.iter_mut().find(|(v, _)| *v == voice) {
                                Some((_, o)) => {
                                    *o += 1;
                                    *o - 1
                                }
                                None => {
                                    orders.push((voice.clone(), 1));
                                    0
                                }
                            };
                            let ties: Vec<&str> = c
                                .children()
                                .filter(|x| x.has_tag_name("tie"))
                                .filter_map(|x| x.attribute("type"))
                                .collect();
                            notes.push(WrittenNote {
                                id: WrittenNoteId {
                                    part: part_id.clone(),
                                    measure_index: index,
                                    voice,
                                    order,
                                },
                                measure_number: measure.number.clone(),
                                staff: child_text(c, "staff")
                                    .map_or(Ok(1), |s| parse_int(Some(s), "staff"))?,
                                pitch: midi_pitch(p)?,
                                offset: start,
                                duration,
                                tie_start: ties.contains(&"start"),
                                tie_stop: ties.contains(&"stop"),
                                is_grace,
                            });
                        }
                    } else if !is_rest {
                        issue("note", "音高のない音符（読まない）".into());
                    }
                    if !is_chord && !is_grace {
                        last_start = cursor;
                        cursor = cursor + duration;
                        end = end.max(cursor);
                    }
                }
                "barline" => read_barline(c, &mut measure, &mut issue),
                "direction" | "sound" => {
                    for s in c.descendants().filter(|s| s.has_tag_name("sound")) {
                        read_sound(s, &mut measure, &mut issue);
                    }
                }
                n if IGNORED_MEASURE_CHILDREN.contains(&n) => {}
                n => issue(n, "measure の中の読まない要素".into()),
            }
        }
        measure.length = if end == Quarters::ZERO {
            time_length
        } else {
            end
        };
        measures.push(measure);
    }
    Ok(Score {
        part: part_id,
        measures,
        notes,
        issues,
    })
}

fn to_quarters(duration: i64, divisions: Option<i64>) -> Result<Quarters, ReadError> {
    let d = divisions.ok_or_else(|| ReadError("divisions より前に duration がある".into()))?;
    if d <= 0 {
        return Err(ReadError(format!("divisions が正でない: {d}")));
    }
    Ok(Quarters::new(duration, d))
}

fn midi_pitch(p: roxmltree::Node) -> Result<i32, ReadError> {
    let step = match child_text(p, "step") {
        Some("C") => 0,
        Some("D") => 2,
        Some("E") => 4,
        Some("F") => 5,
        Some("G") => 7,
        Some("A") => 9,
        Some("B") => 11,
        s => return Err(ReadError(format!("step が読めない: {s:?}"))),
    };
    let octave: i32 = parse_int(child_text(p, "octave"), "octave")?;
    // 微分音（小数の alter）は扱わない。整数でなければ誤りにする
    let alter: i32 = child_text(p, "alter").map_or(Ok(0), |s| parse_int(Some(s), "alter"))?;
    Ok((octave + 1) * 12 + step + alter)
}

fn read_barline(b: roxmltree::Node, m: &mut Measure, issue: &mut impl FnMut(&str, String)) {
    for c in b.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "bar-style" => {}
            "repeat" => match c.attribute("direction") {
                Some("forward") => m.repeat_forward = true,
                Some("backward") => {
                    m.repeat_backward = Some(
                        c.attribute("times")
                            .and_then(|t| t.parse().ok())
                            .unwrap_or(2),
                    );
                }
                d => issue("repeat", format!("向きが読めない: {d:?}")),
            },
            "ending" => match c.attribute("type") {
                Some("start") => {
                    let nums: Option<Vec<u32>> = c
                        .attribute("number")
                        .unwrap_or("")
                        .split([',', ' '])
                        .filter(|s| !s.is_empty())
                        .map(|s| s.parse().ok())
                        .collect();
                    match nums {
                        Some(n) if !n.is_empty() => m.ending_start = Some(n),
                        _ => issue(
                            "ending",
                            format!("番号が読めない: {:?}", c.attribute("number")),
                        ),
                    }
                }
                Some("stop") | Some("discontinue") => m.ending_end = true,
                t => issue("ending", format!("type が読めない: {t:?}")),
            },
            n => issue(n, "barline の中の読まない要素".into()),
        }
    }
}

fn read_sound(s: roxmltree::Node, m: &mut Measure, issue: &mut impl FnMut(&str, String)) {
    for a in s.attributes() {
        let v = Some(a.value().to_string());
        match a.name() {
            "segno" => m.segno = v,
            "coda" => m.coda = v,
            "tocoda" => m.tocoda = v,
            "dalsegno" => m.dalsegno = v,
            n if IGNORED_SOUND_ATTRIBUTES.contains(&n) => {}
            n => issue(
                &format!("sound@{n}"),
                format!("読まない sound の属性（値 {}）", a.value()),
            ),
        }
    }
    for c in s.children().filter(|c| c.is_element()) {
        issue(c.tag_name().name(), "sound の中の読まない要素".into());
    }
}

#[cfg(test)]
mod tests {
    use super::read_score;

    #[test]
    fn unread_elements_are_listed() {
        let xml = r#"<?xml version="1.0"?>
<score-partwise><part-list/><part id="P1"><measure number="1">
  <attributes><divisions>1</divisions><transpose><chromatic>-2</chromatic></transpose></attributes>
  <harmony><root><root-step>C</root-step></root></harmony>
  <note><unpitched><display-step>C</display-step><display-octave>4</display-octave></unpitched><duration>1</duration></note>
  <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><lyric><text>la</text></lyric></note>
  <direction><direction-type><words>Fine</words></direction-type><sound fine="yes" tempo="90"/></direction>
</measure></part><part id="P2"/></score-partwise>"#;
        let score = read_score(xml.as_bytes()).expect("読めるはず").score;
        let got: Vec<(Option<usize>, &str)> = score
            .issues
            .iter()
            .map(|i| (i.measure_index, i.element.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (None, "part"),
                (Some(0), "transpose"),
                (Some(0), "harmony"),
                (Some(0), "unpitched"),
                (Some(0), "note"),
                (Some(0), "lyric"),
                (Some(0), "sound@fine"),
            ]
        );
        assert_eq!(score.notes.len(), 1);
    }

    #[test]
    fn broken_xml_is_an_error() {
        assert!(read_score(b"<score-partwise>").is_err());
        assert!(read_score(b"<score-timewise/>").is_err());
    }
}
