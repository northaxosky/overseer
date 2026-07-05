//! Tests for INI parsing, merging, and key edits

use super::*;
use crate::instance::Instance;
use camino::Utf8Path;

// --- parser ---

#[test]
fn parses_sections_and_keys() {
    let ini = Ini::parse("[General]\nsFoo=Bar\n[Archive]\nbInvalidateOlderFiles=1\n");
    assert_eq!(ini.get("General", "sFoo"), Some("Bar"));
    assert_eq!(ini.get("Archive", "bInvalidateOlderFiles"), Some("1"));
}

#[test]
fn a_leading_utf8_bom_is_ignored() {
    // Windows editors often save INIs with a BOM; without stripping it the first; `[section]` header is misread and every key under it is lost
    let ini = Ini::parse("\u{FEFF}[Archive]\nbInvalidateOlderFiles=1\n");
    assert_eq!(ini.get("Archive", "bInvalidateOlderFiles"), Some("1"));
}

#[test]
fn section_and_key_lookups_are_case_insensitive() {
    let ini = Ini::parse("[ARCHIVE]\nSResourceDataDirsFinal=STRINGS\\\n");
    assert_eq!(
        ini.get("archive", "sresourcedatadirsfinal"),
        Some("STRINGS\\")
    );
    assert_eq!(
        ini.get("Archive", "SResourceDataDirsFinal"),
        Some("STRINGS\\")
    );
}

#[test]
fn values_keep_their_casing_and_inner_equals() {
    // split_once('=') splits on the first '=' only, so a value with '=' survives
    let ini = Ini::parse("[General]\nsKey=A=B=C\n");
    assert_eq!(ini.get("general", "skey"), Some("A=B=C"));
}

#[test]
fn blank_and_comment_lines_are_ignored() {
    let ini = Ini::parse("\n; a comment\n[General]\n\n; another\nsFoo=1\n");
    assert_eq!(ini.get("general", "sFoo"), Some("1"));
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    let ini = Ini::parse("  [General]  \n  sFoo  =  bar baz \n");
    assert_eq!(ini.get("general", "sfoo"), Some("bar baz"));
}

#[test]
fn missing_keys_and_sections_return_none() {
    let ini = Ini::parse("[General]\nsFoo=1\n");
    assert_eq!(ini.get("general", "missing"), None);
    assert_eq!(ini.get("nope", "sFoo"), None);
}

// --- merge ---

#[test]
fn merge_lets_the_other_file_win() {
    let mut base = Ini::parse("[Archive]\nsResourceDataDirsFinal=STRINGS\\\nbKeep=1\n");
    base.merge(Ini::parse("[Archive]\nsResourceDataDirsFinal=\n"));
    // The shared key is overridden...
    assert_eq!(base.get("archive", "sResourceDataDirsFinal"), Some(""));
    // ...but a key the other file doesn't mention is left alone
    assert_eq!(base.get("archive", "bKeep"), Some("1"));
}

#[test]
fn merge_adds_new_sections() {
    let mut base = Ini::parse("[General]\nsFoo=1\n");
    base.merge(Ini::parse("[Archive]\nbBar=1\n"));
    assert_eq!(base.get("general", "sFoo"), Some("1"));
    assert_eq!(base.get("archive", "bBar"), Some("1"));
}

// --- ini_dir + read_game_inis (driven through the ini_dir override) ---

use crate::test_support::temp;

fn instance_with_ini_dir(ini_dir: &Utf8Path) -> Instance {
    let mut instance = Instance::new("inst", "game");
    instance.config.ini_dir = Some(ini_dir.to_owned());
    instance
}

#[test]
fn resolve_uses_the_override_when_set() {
    let (_t, base) = temp();
    let instance = instance_with_ini_dir(&base);
    assert_eq!(instance.ini_dir().unwrap(), base);
}

#[test]
fn reads_and_merges_the_game_inis() {
    let (_t, dir) = temp();
    // The default game is Fallout4, so stem = "Fallout4"
    std::fs::write(
        dir.join("Fallout4.ini"),
        "[Archive]\nsResourceDataDirsFinal=STRINGS\\\nbInvalidateOlderFiles=0\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Fallout4Custom.ini"),
        "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    )
    .unwrap();
    std::fs::write(dir.join("Fallout4Prefs.ini"), "[NVFlex]\nbNVFlexEnable=1\n").unwrap();

    let inis = read_game_inis(&instance_with_ini_dir(&dir)).unwrap();
    // Custom overrides base within `settings`
    assert_eq!(
        inis.settings.get("archive", "bInvalidateOlderFiles"),
        Some("1")
    );
    assert_eq!(
        inis.settings.get("archive", "sResourceDataDirsFinal"),
        Some("")
    );
    // Prefs is kept separate
    assert_eq!(inis.prefs.get("nvflex", "bNVFlexEnable"), Some("1"));
}

#[test]
fn missing_ini_files_parse_as_empty() {
    let (_t, dir) = temp(); // nothing written
    let inis = read_game_inis(&instance_with_ini_dir(&dir)).unwrap();
    assert_eq!(inis.settings.get("archive", "bInvalidateOlderFiles"), None);
    assert_eq!(inis.prefs.get("nvflex", "bNVFlexEnable"), None);
}

// --- set_key / unset_key (surgical, content-preserving edits) ---

#[test]
fn set_key_replaces_an_existing_value_in_place() {
    let out = set_key(
        "[General]\r\nSLocalSavePath=Saves\\\r\n",
        "General",
        "SLocalSavePath",
        "Saves\\Hardcore\\",
    );
    assert_eq!(out, "[General]\r\nSLocalSavePath=Saves\\Hardcore\\");
}

#[test]
fn set_key_appends_to_an_existing_section() {
    let out = set_key(
        "[General]\r\nuGridsToLoad=5\r\n",
        "General",
        "SLocalSavePath",
        "Saves\\P\\",
    );
    assert_eq!(
        out,
        "[General]\r\nuGridsToLoad=5\r\nSLocalSavePath=Saves\\P\\"
    );
}

#[test]
fn set_key_creates_a_missing_section_at_eof() {
    let out = set_key(
        "[Display]\r\niSize=1\r\n",
        "General",
        "SLocalSavePath",
        "Saves\\P\\",
    );
    assert_eq!(
        out,
        "[Display]\r\niSize=1\r\n[General]\r\nSLocalSavePath=Saves\\P\\"
    );
}

#[test]
fn set_key_into_empty_text_creates_the_section() {
    assert_eq!(
        set_key("", "General", "SLocalSavePath", "Saves\\P\\"),
        "[General]\r\nSLocalSavePath=Saves\\P\\"
    );
}

#[test]
fn set_key_preserves_other_sections_keys_and_comments() {
    // The regression we care about: injecting a save path must not disturb the; user's archive-invalidation block or their comments
    let original = "; my setup\r\n[General]\r\nuGridsToLoad=5\r\n\r\n[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n";
    let out = set_key(original, "General", "SLocalSavePath", "Saves\\P\\");

    // Re-parse: every original setting survives, plus our new key
    let ini = Ini::parse(&out);
    assert_eq!(ini.get("general", "SLocalSavePath"), Some("Saves\\P\\"));
    assert_eq!(ini.get("general", "uGridsToLoad"), Some("5"));
    assert_eq!(ini.get("archive", "bInvalidateOlderFiles"), Some("1"));
    assert_eq!(ini.get("archive", "sResourceDataDirsFinal"), Some(""));
    // The comment line is carried through verbatim
    assert!(out.contains("; my setup"), "comment preserved: {out:?}");
    // Our key lands inside [General] (before the [Archive] header), not leaked into [Archive]
    let save_at = out.find("SLocalSavePath").expect("key present");
    let archive_at = out.find("[Archive]").expect("archive header present");
    assert!(
        save_at < archive_at,
        "save key must sit in [General]: {out:?}"
    );
}

#[test]
fn section_and_key_matching_is_case_insensitive() {
    // Existing header/key in a different case is still found and replaced (no duplicate)
    let out = set_key(
        "[general]\r\nslocalsavepath=old\r\n",
        "General",
        "SLocalSavePath",
        "new",
    );
    assert_eq!(out, "[general]\r\nSLocalSavePath=new");
}

#[test]
fn unset_key_removes_only_the_target_line() {
    let out = unset_key(
        "[General]\r\nuGridsToLoad=5\r\nSLocalSavePath=Saves\\P\\\r\n",
        "General",
        "SLocalSavePath",
    );
    assert_eq!(out, "[General]\r\nuGridsToLoad=5");
}

#[test]
fn unset_key_is_a_noop_when_absent() {
    let original = "[General]\r\nuGridsToLoad=5";
    assert_eq!(unset_key(original, "General", "SLocalSavePath"), original);
}

#[test]
fn unset_key_ignores_a_same_named_key_in_another_section() {
    // A key with the same name under a different section must not be touched
    let out = unset_key(
        "[General]\r\nSLocalSavePath=ours\r\n[Other]\r\nSLocalSavePath=theirs\r\n",
        "General",
        "SLocalSavePath",
    );
    assert_eq!(out, "[General]\r\n[Other]\r\nSLocalSavePath=theirs");
}

#[test]
fn set_then_unset_round_trips_to_the_original_content() {
    // Deploy then purge: the user's settings are back, our key is gone
    let original = "[General]\r\nuGridsToLoad=5\r\n[Archive]\r\nbInvalidateOlderFiles=1\r\n";
    let injected = set_key(original, "General", "SLocalSavePath", "Saves\\P\\");
    let restored = unset_key(&injected, "General", "SLocalSavePath");

    let ini = Ini::parse(&restored);
    assert_eq!(ini.get("general", "SLocalSavePath"), None);
    assert_eq!(ini.get("general", "uGridsToLoad"), Some("5"));
    assert_eq!(ini.get("archive", "bInvalidateOlderFiles"), Some("1"));
}

#[test]
fn set_key_preserves_a_pure_lf_files_newlines() {
    // An LF-only INI must stay LF, not be rewritten to CRLF
    let out = set_key(
        "[General]\nuGridsToLoad=5\n",
        "General",
        "SLocalSavePath",
        "Saves\\P\\",
    );
    assert_eq!(out, "[General]\nuGridsToLoad=5\nSLocalSavePath=Saves\\P\\");
}

#[test]
fn unset_key_preserves_a_pure_lf_files_newlines() {
    let out = unset_key(
        "[General]\nuGridsToLoad=5\nSLocalSavePath=x\n",
        "General",
        "SLocalSavePath",
    );
    assert_eq!(out, "[General]\nuGridsToLoad=5");
}
