use lyrics_parser::parser::{
    auto_parser::AutoParser, enhanced_lrc_parser::EnhancedLrcParser,
    kugou_krc_parser::KugouKrcParser, lyricify_syllable_parser::LyricifySyllableParser,
    lyrics_parser::LyricsParser, netease_yrc_parser::NeteaseYrcParser, ttml_parser::TtmlParser,
};
use lyrics_parser::{KaraokeAlignment, SyncedLineKind};

#[test]
fn enhanced_lrc_parses_plain_lrc_and_metadata() {
    let content = "[ti:Song]\n[ar:Artist]\n[00:01.50][00:03.000]Hello\n";
    let lyrics = EnhancedLrcParser.parse(content);

    assert_eq!(lyrics.title, "Song");
    assert_eq!(lyrics.artists[0].name, "Artist");
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[0].start(), 1500);
    assert_eq!(lyrics.lines[1].start(), 3000);
}

#[test]
fn lyricify_syllable_parser_preserves_alignment() {
    let content = "[2]Hel(100,200)lo(300,400)";
    let lyrics = LyricifySyllableParser.parse(content);

    let SyncedLineKind::MainKaraoke(line) = &lyrics.lines[0] else {
        panic!("expected karaoke");
    };
    assert_eq!(line.alignment, KaraokeAlignment::End);
    assert_eq!(line.syllables[0].content, "Hel");
    assert_eq!(line.syllables[1].end, 700);
}

#[test]
fn netease_yrc_parser_normalizes_relative_syllable_times() {
    let content = "[12580,3470](0,250,0)难(250,300,0)以";
    let lyrics = NeteaseYrcParser.parse(content);

    let SyncedLineKind::MainKaraoke(line) = &lyrics.lines[0] else {
        panic!("expected karaoke");
    };
    assert_eq!(line.syllables[0].start, 12580);
    assert_eq!(line.syllables[1].end, 13130);
}

#[test]
fn kugou_krc_parser_parses_word_offsets() {
    let content = "[1000,1000]<0,300,0>你<300,300,0>好";
    let lyrics = KugouKrcParser.parse(content);

    let SyncedLineKind::MainKaraoke(line) = &lyrics.lines[0] else {
        panic!("expected karaoke");
    };
    assert_eq!(line.syllables[0].content, "你");
    assert_eq!(line.syllables[0].start, 1000);
    assert_eq!(line.syllables[1].end, 1600);
}

#[test]
fn ttml_parser_reads_span_syllables() {
    let content = r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div><p begin="00:01.000" end="00:02.000" itunes:key="L1"><span begin="00:01.000" end="00:01.500">Hel</span><span begin="00:01.500" end="00:02.000">lo</span></p></div></body></tt>"#;
    let lyrics = TtmlParser::default().parse(content);

    let SyncedLineKind::MainKaraoke(line) = &lyrics.lines[0] else {
        panic!("expected karaoke");
    };
    assert_eq!(line.start, 1000);
    assert_eq!(line.syllables[0].content, "Hel");
}

#[test]
fn auto_parser_keeps_lyrics_core_order() {
    let auto = AutoParser::default();
    let ttml = r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div><p begin="0" end="1">x</p></div></body></tt>"#;
    assert!(auto.can_parse(ttml));
    let lyrics = auto.parse("[00:00.00]Hello");
    assert_eq!(lyrics.lines.len(), 1);
}
