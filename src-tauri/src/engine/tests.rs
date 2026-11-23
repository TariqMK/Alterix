//! Engine tests for every step, option and edge case.

use super::*;

fn items(dir: &str, files: &[&str]) -> Vec<Item> {
    files
        .iter()
        .map(|f| Item {
            path: Path::new(dir).join(f).to_string_lossy().into_owned(),
            is_dir: false,
            root: dir.into(),
        })
        .collect()
}

fn run_opts(files: &[&str], rules: Vec<Rule>, opts: &Options) -> Vec<PreviewItem> {
    let cache = InfoCache::default();
    let trace = HashSet::new();
    let env = Env {
        opts,
        cache: &cache,
        script: None,
        trace: &trace,
    };
    preview(&items("/nonexistent/dir", files), &rules, &env).items
}

fn run(files: &[&str], rules: Vec<Rule>) -> Vec<String> {
    run_opts(files, rules, &Options::default())
        .into_iter()
        .map(|p| p.new_name)
        .collect()
}

fn one(file: &str, rule: Rule) -> String {
    run(&[file], vec![rule]).remove(0)
}

fn pair(find: &str, with: &str) -> ReplacePair {
    ReplacePair {
        find: find.into(),
        with: with.into(),
    }
}

fn replace(find: &str, with: &str) -> Rule {
    Rule::Replace {
        pairs: vec![pair(find, with)],
        match_case: false,
    }
}

fn remove() -> Rule {
    Rule::Remove {
        first: 0,
        last: 0,
        from: 0,
        to: 0,
        chars: String::new(),
        words: String::new(),
        crop: CropMode::None,
        crop_text: String::new(),
        digits: false,
        high: false,
        trim: false,
        double_spaces: false,
        accents: false,
        all_chars: false,
        symbols: false,
        lead_dots: LeadDots::None,
    }
}

fn numbering(position: NumPosition, start: i64, pad: usize, sep: &str) -> Rule {
    Rule::Numbering {
        position,
        at: 0,
        start,
        step: 1,
        pad,
        separator: sep.into(),
        break_at: 0,
        per_folder: false,
        num_type: NumType::Decimal,
        base: 10,
        letter_case: LetterCase::Default,
    }
}

#[test]
fn split_name_keeps_dotfiles_and_last_extension() {
    assert_eq!(split_name("photo.jpg"), ("photo", ".jpg"));
    assert_eq!(split_name("archive.tar.gz"), ("archive.tar", ".gz"));
    assert_eq!(split_name(".bashrc"), (".bashrc", ""));
}

#[test]
fn regex_step_standard_and_include_ext() {
    let r = Rule::Regex {
        pattern: r"^([A-Z][a-z]*) ([A-Z][a-z]*)".into(),
        replace: r"\2 \1".into(),
        include_ext: false,
        mode: RegexMode::Standard,
    };
    assert_eq!(one("Program Files.txt", r), "Files Program.txt");
    let r = Rule::Regex {
        pattern: r"\.txt$".into(),
        replace: ".md".into(),
        include_ext: true,
        mode: RegexMode::V2,
    };
    assert_eq!(one("notes.txt", r), "notes.md");
}

#[test]
fn name_modes() {
    let name = |mode, v: &str, f: &str| {
        one(
            f,
            Rule::Name {
                mode,
                value: v.into(),
            },
        )
    };
    assert_eq!(name(NameMode::Reverse, "", "12345.txt"), "54321.txt");
    assert_eq!(
        name(NameMode::Fixed, "Photo <counter{pad0:2}>", "x.jpg"),
        "Photo 01.jpg"
    );
    assert_eq!(name(NameMode::Remove, "", "x.jpg"), ".jpg");
    assert_eq!(
        name(NameMode::PadNumbers, "5", "IMG_7_final.jpg"),
        "IMG_00007_final.jpg"
    );
    assert_eq!(
        name(NameMode::PadNumbers, "5>A", "IMG_7.jpg"),
        "IMG_AAAA7.jpg"
    );
    assert_eq!(
        name(NameMode::ReformatDate, "", "Trip 14.06.2024.jpg"),
        "Trip 2024-06-14.jpg"
    );
    assert_eq!(name(NameMode::Fixed, "<name{upper}>", "abc.jpg"), "ABC.jpg");
}

#[test]
fn replace_syntax() {
    assert_eq!(one("1 and 2.txt", replace("1|2", "3|4")), "3 and 4.txt");
    assert_eq!(one("1 and 2.txt", replace("1|2", "3")), "3 and 3.txt");
    assert_eq!(one("a1|2b.txt", replace(r"1\|2", "3")), "a3b.txt");
    assert_eq!(one("aaa.txt", replace(r"\first\a", "b")), "baa.txt");
    assert_eq!(
        one("text text text.txt", replace(r"\last\text", "")),
        "text text .txt"
    );
    assert_eq!(
        one("text text.txt", replace(r"\second\text", "txt")),
        "text txt.txt"
    );
    assert_eq!(
        one("xtext text.txt", replace(r"\start\text", "txt")),
        "xtext text.txt"
    );
    assert_eq!(
        one("Copy (12) report.txt", replace("Copy (*)", "C [*]")),
        "C [12] report.txt"
    );
    assert_eq!(
        one("Copy (1) report.txt", replace("Copy (?)", "Cp [?]")),
        "Cp [1] report.txt"
    );
    assert_eq!(
        one("Copy (12) report.txt", replace("\"Copy (*)\"", "*")),
        "Copy (12) report.txt"
    );
    assert_eq!(one("Copy (12).txt", replace("\"Copy (*)\"", "*")), "12.txt");
    assert_eq!(
        one("a12345b.txt", replace(r"\regex\(\d{5})", "A$1")),
        "aA12345b.txt"
    );
    let cs = Rule::Replace {
        pairs: vec![pair("abc", "x")],
        match_case: true,
    };
    assert_eq!(one("ABC abc.txt", cs), "ABC x.txt");
    let many = Rule::Replace {
        pairs: vec![pair("a", "b"), pair("b", "c")],
        match_case: false,
    };
    assert_eq!(one("a.txt", many), "c.txt");
}

#[test]
fn case_step_does_not_touch_extension() {
    assert_eq!(
        one(
            "joe public.TXT",
            Rule::Case {
                mode: CaseMode::Title,
                exceptions: String::new()
            }
        ),
        "Joe Public.TXT"
    );
}

#[test]
fn remove_step_examples() {
    let mut r = remove();
    if let Rule::Remove { first, .. } = &mut r {
        *first = 2;
    }
    assert_eq!(one("Joe Public.txt", r), "e Public.txt");
    let mut r = remove();
    if let Rule::Remove { last, .. } = &mut r {
        *last = 2;
    }
    assert_eq!(one("Joe Public.txt", r), "Joe Publ.txt");
    let mut r = remove();
    if let Rule::Remove { from, to, .. } = &mut r {
        *from = -3;
        *to = -2;
    }
    assert_eq!(one("abcdef.txt", r), "abcf.txt");
    let mut r = remove();
    if let Rule::Remove {
        crop, crop_text, ..
    } = &mut r
    {
        *crop = CropMode::Special;
        *crop_text = "[*]".into();
    }
    assert_eq!(one("Hello[ABC] Joe.txt", r), "Hello Joe.txt");
    for (spec, want) in [("_*", "report"), ("*_", "2025_final")] {
        let mut r = remove();
        if let Rule::Remove {
            crop, crop_text, ..
        } = &mut r
        {
            *crop = CropMode::Special;
            *crop_text = spec.into();
        }
        assert_eq!(one("report_2025_final.txt", r), format!("{want}.txt"));
    }
    for (mode, want) in [
        (CropMode::After, "report_"),
        (CropMode::Before, "_2025_final"),
    ] {
        let mut r = remove();
        if let Rule::Remove {
            crop, crop_text, ..
        } = &mut r
        {
            *crop = mode;
            *crop_text = "_".into();
        }
        assert_eq!(one("report_2025_final.txt", r), format!("{want}.txt"));
    }
    let mut r = remove();
    if let Rule::Remove {
        digits,
        trim,
        double_spaces,
        symbols,
        accents,
        lead_dots,
        ..
    } = &mut r
    {
        *digits = true;
        *trim = true;
        *double_spaces = true;
        *symbols = true;
        *accents = true;
        *lead_dots = LeadDots::All;
    }
    assert_eq!(one("..  Café  (2024)!  .txt", r), "Cafe.txt");
}

#[test]
fn removed_tag_puts_text_back() {
    let mut r = remove();
    if let Rule::Remove { first, .. } = &mut r {
        *first = 4;
    }
    let add = Rule::Add {
        prefix: String::new(),
        insert: String::new(),
        at: 0,
        suffix: " <removed>".into(),
        word_space: false,
    };
    assert_eq!(
        run(&["IMG_holiday.jpg"], vec![r, add]),
        vec!["holiday IMG_.jpg"]
    );
}

#[test]
fn move_copy_step() {
    let mc = |action, count, target, sep: &str| Rule::MoveCopy {
        action,
        count,
        from: 0,
        to: 0,
        target,
        position: 0,
        separator: sep.into(),
    };
    assert_eq!(
        one(
            "report 123456.txt",
            mc(MoveAction::MoveLast, 6, Target::Start, " ")
        ),
        "123456 report .txt"
    );
    assert_eq!(
        one("abc.txt", mc(MoveAction::CopyFirst, 1, Target::End, "-")),
        "abc-a.txt"
    );
    let range = Rule::MoveCopy {
        action: MoveAction::MoveRange,
        count: 0,
        from: 2,
        to: 3,
        target: Target::Position,
        position: -1,
        separator: String::new(),
    };
    assert_eq!(one("abcde.txt", range), "adbce.txt");
}

#[test]
fn add_step() {
    let add = Rule::Add {
        prefix: "A ".into(),
        insert: "-".into(),
        at: 3,
        suffix: " Z".into(),
        word_space: true,
    };
    assert_eq!(one("MyHoliday.jpg", add), "A My-Holiday Z.jpg");
}

#[test]
fn folder_name_levels() {
    let f = |levels, pos| Rule::FolderName {
        position: pos,
        separator: "-".into(),
        levels,
    };
    let p = &["Cat.jpg"];
    assert_eq!(run(p, vec![f(1, Position::Prefix)]), vec!["dir-Cat.jpg"]);
    assert_eq!(
        run(p, vec![f(2, Position::Prefix)]),
        vec!["nonexistent-dir-Cat.jpg"]
    );
    assert_eq!(
        run(p, vec![f(-2, Position::Prefix)]),
        vec!["nonexistent-Cat.jpg"]
    );
    assert_eq!(run(p, vec![f(1, Position::Suffix)]), vec!["Cat-dir.jpg"]);
}

#[test]
fn numbering_modes() {
    assert_eq!(
        run(
            &["b.jpg", "a.jpg"],
            vec![numbering(NumPosition::Suffix, 1, 4, "-")]
        ),
        vec!["b-0001.jpg", "a-0002.jpg"]
    );
    assert_eq!(
        run(
            &["x.jpg", "y.jpg"],
            vec![numbering(NumPosition::Prefix, 1, 0, "ABC:DEF:")]
        ),
        vec!["ABC1DEF1x.jpg", "ABC2DEF2y.jpg"]
    );
    assert_eq!(
        run(&["x.jpg"], vec![numbering(NumPosition::Both, 7, 0, "_")]),
        vec!["7_x_7.jpg"]
    );
    let mut r = numbering(NumPosition::Prefix, 26, 0, " ");
    if let Rule::Numbering { num_type, base, .. } = &mut r {
        *num_type = NumType::Base;
        *base = 16;
    }
    assert_eq!(one("x.jpg", r), "1A x.jpg");
    let mut r = numbering(NumPosition::Prefix, 4, 0, " ");
    if let Rule::Numbering { num_type, .. } = &mut r {
        *num_type = NumType::Roman;
    }
    assert_eq!(one("x.jpg", r), "IV x.jpg");
    let mut r = numbering(NumPosition::Prefix, 1, 3, " ");
    if let Rule::Numbering { num_type, .. } = &mut r {
        *num_type = NumType::LettersUpper;
    }
    assert_eq!(one("x.jpg", r), "AAB x.jpg");
    let mut r = numbering(NumPosition::Insert, 1, 0, "");
    if let Rule::Numbering { at, .. } = &mut r {
        *at = 2;
    }
    assert_eq!(one("xy.jpg", r), "x1y.jpg");
}

#[test]
fn numbering_break_resets_on_character_change() {
    let mut r = numbering(NumPosition::Suffix, 1, 0, " ");
    if let Rule::Numbering { break_at, .. } = &mut r {
        *break_at = 1;
    }
    assert_eq!(
        run(&["a1.jpg", "a2.jpg", "b1.jpg"], vec![r]),
        vec!["a1 1.jpg", "a2 2.jpg", "b1 1.jpg"]
    );
}

#[test]
fn extension_modes() {
    let e = |mode, v: &str, f: &str| {
        one(
            f,
            Rule::Extension {
                mode,
                value: v.into(),
            },
        )
    };
    assert_eq!(e(ExtMode::Lower, "", "a.JPG"), "a.jpg");
    assert_eq!(e(ExtMode::Title, "", "a.jpeg"), "a.Jpeg");
    assert_eq!(e(ExtMode::Fixed, ".doc", "a.txt"), "a.doc");
    assert_eq!(e(ExtMode::Extra, "gif", "my.holiday"), "my.holiday.gif");
    assert_eq!(e(ExtMode::Remove, "", "My.Holiday"), "My");
}

#[test]
fn character_translations() {
    assert_eq!(
        one(
            "A$.txt",
            Rule::Translate {
                rules: "A=B\n$=D,O,L,L,A,R".into()
            }
        ),
        "BDOLLAR.txt"
    );
    assert_eq!(
        one(
            "ABC.txt",
            Rule::Translate {
                rules: "41,066,C=D,E,070".into()
            }
        ),
        "DEF.txt"
    );
    assert_eq!(
        one(
            "A.txt",
            Rule::Translate {
                rules: "41=066".into()
            }
        ),
        "B.txt"
    );
}

#[test]
fn name_segment_limits_later_steps() {
    let seg = |f: &str, t: &str| Rule::Segment {
        from: f.into(),
        to: t.into(),
    };
    let upper = Rule::Case {
        mode: CaseMode::Upper,
        exceptions: String::new(),
    };
    assert_eq!(
        run(&["abcdefgh.txt"], vec![seg("1", "5"), upper.clone()]),
        vec!["ABCDEfgh.txt"]
    );
    assert_eq!(
        run(&["abcdefgh.txt"], vec![seg("-3", "-2"), upper.clone()]),
        vec!["abcdeFGh.txt"]
    );
    assert_eq!(
        run(&["a-bc-d.txt"], vec![seg("-", "-"), upper.clone()]),
        vec!["a-BC-d.txt"]
    );
    assert_eq!(
        run(&["x(ab)y.txt"], vec![seg("(", ")"), upper.clone()]),
        vec!["x(AB)y.txt"]
    );
    assert_eq!(
        run(&["a_b_c_d.txt"], vec![seg("", "_,2"), upper.clone()]),
        vec!["A_B_c_d.txt"]
    );
    assert_eq!(
        run(&["a_b_c_d.txt"], vec![seg("_", "_,3"), upper]),
        vec!["a_B_C_d.txt"]
    );
}

#[test]
fn trace_records_each_step() {
    let cache = InfoCache::default();
    let opts = Options::default();
    let list = items("/nonexistent/dir", &["a b.txt"]);
    let trace: HashSet<String> = [list[0].path.clone()].into();
    let env = Env {
        opts: &opts,
        cache: &cache,
        script: None,
        trace: &trace,
    };
    let rules = vec![
        replace(" ", "_"),
        Rule::Case {
            mode: CaseMode::Upper,
            exceptions: String::new(),
        },
    ];
    let t = preview(&list, &rules, &env).items[0].trace.clone().unwrap();
    assert_eq!(t[0].name, "a_b.txt");
    assert_eq!(t[1].name, "A_B.txt");
}

#[test]
fn rules_run_in_order() {
    let rules = vec![
        replace(" ", "_"),
        Rule::Case {
            mode: CaseMode::Lower,
            exceptions: String::new(),
        },
    ];
    assert_eq!(run(&["My File.txt"], rules), vec!["my_file.txt"]);
}

#[test]
fn duplicate_names_are_conflicts_or_get_counters() {
    let strip = || Rule::Regex {
        pattern: r"\d".into(),
        replace: String::new(),
        include_ext: false,
        mode: RegexMode::V2,
    };
    let strip_all = || Rule::Regex {
        pattern: r"\d/g".into(),
        replace: String::new(),
        include_ext: false,
        mode: RegexMode::V2,
    };
    let out = run_opts(
        &["a1.txt", "a2.txt", "b.txt"],
        vec![strip()],
        &Options::default(),
    );
    assert_eq!(out[0].status, Status::Conflict);
    assert_eq!(out[1].status, Status::Conflict);
    assert_eq!(out[2].status, Status::Unchanged);
    let opts = Options {
        prevent_duplicates: true,
        ..Options::default()
    };
    let out = run_opts(&["a1.txt", "a2.txt", "a3.txt"], vec![strip_all()], &opts);
    let names: Vec<_> = out.iter().map(|p| p.new_name.as_str()).collect();
    assert_eq!(names, ["a.txt", "a_1.txt", "a_2.txt"]);
    assert!(out.iter().all(|p| p.status == Status::Changed));
    let opts = Options {
        prevent_duplicates: true,
        duplicate_format: "Copy (%count%) of %name%".into(),
        ..Options::default()
    };
    let out = run_opts(&["a1.txt", "a2.txt"], vec![strip_all()], &opts);
    assert_eq!(out[1].new_name, "Copy (1) of a.txt");
}

#[test]
fn duplicate_formats() {
    assert_eq!(duplicate_name("%name%_%count00%", "x", 3), "x_03");
    assert_eq!(duplicate_name("%name%%countA%", "x", 2), "xB");
    assert_eq!(duplicate_name("%count000%%name%", "x", 1), "001x");
}

#[test]
fn existing_file_outside_batch_is_a_conflict_or_skipped() {
    let dir = std::env::temp_dir().join(format!("alterix-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("taken.txt"), "").unwrap();
    let list = vec![Item {
        path: dir.join("free.txt").to_string_lossy().into_owned(),
        is_dir: false,
        root: String::new(),
    }];
    let cache = InfoCache::default();
    let trace = HashSet::new();
    let rules = vec![replace("free", "taken")];
    let opts = Options::default();
    let a = preview(
        &list,
        &rules,
        &Env {
            opts: &opts,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    )
    .items;
    let skip = Options {
        skip_existing: true,
        ..Options::default()
    };
    let b = preview(
        &list,
        &rules,
        &Env {
            opts: &skip,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    )
    .items;
    let dup = Options {
        prevent_duplicates: true,
        ..Options::default()
    };
    let c = preview(
        &list,
        &rules,
        &Env {
            opts: &dup,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    )
    .items;
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(a[0].status, Status::Conflict);
    assert_eq!(b[0].status, Status::Skipped);
    assert_eq!(c[0].new_name, "taken_1.txt");
}

#[test]
fn case_only_change_is_allowed() {
    let out = run_opts(
        &["readme.md"],
        vec![Rule::Case {
            mode: CaseMode::Upper,
            exceptions: String::new(),
        }],
        &Options::default(),
    );
    assert_eq!(out[0].new_name, "README.md");
    assert_eq!(out[0].status, Status::Changed);
}

#[test]
fn unsafe_names_are_invalid() {
    assert!(validate_name("", false).is_some());
    assert!(validate_name("a:b.txt", false).is_some());
    assert!(validate_name("name.", false).is_some());
    assert!(validate_name("con.txt", false).is_some());
    assert!(validate_name("Console.txt", false).is_none());
    let out = run_opts(&["a.txt"], vec![replace("a", "x/y")], &Options::default());
    assert_eq!(out[0].status, Status::Invalid);
}

#[test]
fn folders_in_names_create_subfolders() {
    let opts = Options {
        allow_folders_in_names: true,
        ..Options::default()
    };
    let out = run_opts(&["a.txt"], vec![replace("a", "2024/a")], &opts);
    assert_eq!(out[0].status, Status::Changed);
    assert_eq!(
        Path::new(&out[0].new_path),
        Path::new("/nonexistent/dir/2024/a.txt")
    );
}

#[test]
fn imported_pairs_and_overrides() {
    let opts = Options {
        pairs: vec![pair("TRACK001.mp3", "Headlong.mp3")],
        ..Options::default()
    };
    assert_eq!(
        run_opts(&["Track001.mp3"], vec![], &opts)[0].new_name,
        "Headlong.mp3"
    );
    let mut ov = HashMap::new();
    ov.insert("/nonexistent/dir/x.txt".to_string(), "y.txt".to_string());
    let opts = Options {
        overrides: ov,
        ..Options::default()
    };
    assert_eq!(run_opts(&["x.txt"], vec![], &opts)[0].new_name, "y.txt");
}

#[test]
fn linked_extensions_follow_master() {
    let opts = Options {
        link_master: "jpg".into(),
        link_linked: "raw, png".into(),
        ..Options::default()
    };
    let out = run_opts(
        &["a.jpg", "a.raw", "a.png", "b.raw"],
        vec![numbering(NumPosition::Prefix, 1, 0, "_")],
        &opts,
    );
    let names: Vec<_> = out.iter().map(|p| p.new_name.as_str()).collect();
    assert_eq!(names, ["1_a.jpg", "1_a.raw", "1_a.png", "b.raw"]);
}

#[test]
fn new_location_and_structure() {
    let opts = Options {
        new_location: "/out".into(),
        keep_structure: true,
        ..Options::default()
    };
    let list = vec![Item {
        path: "/src/sub/a.txt".into(),
        is_dir: false,
        root: "/src".into(),
    }];
    let cache = InfoCache::default();
    let trace = HashSet::new();
    let p = preview(
        &list,
        &[],
        &Env {
            opts: &opts,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    )
    .items;
    assert_eq!(Path::new(&p[0].new_path), Path::new("/out/sub/a.txt"));
    let rel = Options {
        new_location: "..".into(),
        ..Options::default()
    };
    let p = preview(
        &list,
        &[],
        &Env {
            opts: &rel,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    )
    .items;
    assert_eq!(Path::new(&p[0].new_path), Path::new("/src/sub/../a.txt"));
}

#[test]
fn retain_autonumber_reports_next_start() {
    let cache = InfoCache::default();
    let opts = Options::default();
    let trace = HashSet::new();
    let list = items("/nonexistent/dir", &["a", "b", "c"]);
    let p = preview(
        &list,
        &[numbering(NumPosition::Prefix, 1, 0, "")],
        &Env {
            opts: &opts,
            cache: &cache,
            script: None,
            trace: &trace,
        },
    );
    assert_eq!(p.next_numbers, vec![(0, 4)]);
}

#[test]
fn timestamps_and_deltas() {
    assert_eq!(
        parse_delta("+1 01:00:00"),
        Some(Duration::days(1) + Duration::hours(1))
    );
    assert_eq!(parse_delta("-0 00:00:30"), Some(-Duration::seconds(30)));
    let info = FileInfo::virtual_file(Path::new("/x"), false);
    let ts = Timestamps {
        modified: StampSet {
            mode: StampMode::Set,
            value: "2024-01-01 10:00:00".into(),
            delta: String::new(),
        },
        increment_seconds: 10,
        ..Timestamps::default()
    };
    let t = new_timestamps(&info, 2, &ts);
    assert_eq!(t[1].unwrap().format("%H:%M:%S").to_string(), "10:00:20");
    assert!(t[0].is_none());
}

#[test]
fn rules_deserialise_from_front_end_json() {
    let json = r#"[
        {"kind":"replace","pairs":[{"find":"a","with":"b"}],"matchCase":false},
        {"kind":"case","mode":"titleEnhanced","exceptions":""},
        {"kind":"numbering","position":"suffix","start":1,"step":1,"pad":2,"separator":"-","numType":"decimal"},
        {"kind":"remove","first":2,"cropText":"x","crop":"special","leadDots":"all"},
        {"kind":"autoDate","position":"prefix","source":"modified","preset":"ymd","separator":"_"},
        {"kind":"segment","from":"1","to":"3"},
        {"kind":"script","code":"newName = name"}
    ]"#;
    let rules: Vec<Rule> = serde_json::from_str(json).unwrap();
    assert_eq!(rules.len(), 7);
    let o: Options = serde_json::from_str(r#"{"preventDuplicates":true}"#).unwrap();
    assert!(o.prevent_duplicates && o.copy);
}

#[test]
fn human_sizes() {
    assert_eq!(human_size(512, "auto"), "512 bytes");
    assert_eq!(human_size(1536, "auto"), "1.5 KB");
    assert_eq!(human_size(10485760, "mb"), "10 MB");
}
