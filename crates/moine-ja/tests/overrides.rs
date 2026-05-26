use moine_core::{damerau_levenshtein_str, distance, distance_with_trace, Lattice};
use moine_ja::{compare_with_overrides, romaji_lattice, OverrideDictionary};

fn symbols_to_string(symbols: &[moine_core::Symbol]) -> String {
    symbols
        .iter()
        .map(|&symbol| char::from_u32(symbol).expect("test symbol should be a char"))
        .collect()
}

fn fixture() -> OverrideDictionary {
    OverrideDictionary::from_yaml_str(include_str!("resources/overrides.yaml"))
        .expect("override fixture should load")
}

#[test]
fn kimetsu_paper_example_matches_by_override_reading() {
    let dict = fixture();
    let left = romaji_lattice("きめつのやいば").expect("kana input should build");
    let right = dict
        .romaji_lattice("鬼滅の刃")
        .expect("override should build");

    assert_eq!(distance(&left, &right), 0);
}

#[test]
fn insatsu_paper_example_has_one_edit_distance() {
    let dict = fixture();
    let left = romaji_lattice("いんさt").expect("kana ascii input should build");
    let right = dict.romaji_lattice("印刷").expect("override should build");
    let trace = distance_with_trace(&left, &right);

    assert_eq!(trace.distance, 1);
    assert_eq!(symbols_to_string(&trace.left_symbols()), "insat");
    assert_eq!(symbols_to_string(&trace.right_symbols()), "insatu");
}

#[test]
fn chadougu_matches_both_short_and_long_vowel_readings() {
    let dict = fixture();
    let right = dict
        .romaji_lattice("茶道具")
        .expect("override should build");

    assert_eq!(distance(&Lattice::from_paths(["chadougu"]), &right), 0);
    assert_eq!(distance(&Lattice::from_paths(["chadoogu"]), &right), 0);
}

#[test]
fn combined_uses_lattice_for_paper_examples() {
    let dict = fixture();

    let kimetsu =
        compare_with_overrides("きめつのやいば", "鬼滅の刃", &dict).expect("should compare");
    assert_eq!(kimetsu.lattice, 0);
    assert!(kimetsu.surface_damerau > kimetsu.lattice);
    assert_eq!(kimetsu.combined, kimetsu.lattice);

    let insatsu = compare_with_overrides("いんさt", "印刷", &dict).expect("should compare");
    assert_eq!(insatsu.lattice, 1);
    assert!(insatsu.surface_damerau > insatsu.lattice);
    assert_eq!(insatsu.combined, insatsu.lattice);

    let chadougu = compare_with_overrides("chadougu", "茶道具", &dict).expect("should compare");
    assert_eq!(chadougu.lattice, 0);
    assert!(chadougu.surface_damerau > chadougu.lattice);
    assert_eq!(chadougu.combined, chadougu.lattice);

    let tokyo = compare_with_overrides("とうきょうと", "東京都", &dict).expect("should compare");
    assert!(tokyo.lattice < tokyo.surface_damerau);
    assert_eq!(tokyo.combined, tokyo.lattice);

    let aichi =
        compare_with_overrides("愛知家コロナ", "愛知県コロナ", &dict).expect("should compare");
    assert!(aichi.lattice <= 1);
    assert!(aichi.combined <= aichi.lattice);
    assert!(aichi.combined <= aichi.surface_damerau);
    assert_eq!(aichi.combined, aichi.lattice);
}

#[test]
fn surface_damerau_covers_transposition_case() {
    let dict = fixture();
    let distances =
        compare_with_overrides("マトリッツォ", "マリトッツォ", &dict).expect("should compare");

    assert_eq!(distances.surface_damerau, 1);
    assert_eq!(damerau_levenshtein_str("マトリッツォ", "マリトッツォ"), 1);
    assert!(distances.surface_damerau < distances.lattice);
    assert_eq!(distances.combined, distances.surface_damerau);
}
