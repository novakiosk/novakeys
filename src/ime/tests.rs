use super::*;
#[test]
#[ignore = "requires installed CJK dictionaries"]
fn real_engine_conversion_and_privacy() {
    let paths = profiles::Paths::temporary();
    let mut engines = Engines::new(paths);
    engines.reset("ja", Mode::Native).unwrap();
    for (raw, reading, word) in [
        ("nihongo", "にほんご", "日本語"),
        ("toukyou", "とうきょう", "東京"),
        ("kanji", "かんじ", "漢字"),
    ] {
        engines.reset("ja", Mode::Native).unwrap();
        let preview = engines
            .command("ja", Mode::Native, &Command::Text(raw.into()))
            .unwrap();
        assert_eq!(preview.preedit, reading);
        let converted = engines
            .command("ja", Mode::Native, &Command::Space)
            .unwrap();
        assert!(
            converted
                .candidates
                .iter()
                .any(|candidate| candidate == word),
            "{raw}: {converted:?}"
        );
        let confirmed = engines
            .command("ja", Mode::Native, &Command::Enter)
            .unwrap();
        assert_eq!(
            confirmed.committed, converted.preedit,
            "{raw}: Enter must confirm conversion"
        );
        assert_eq!(
            confirmed.control, None,
            "{raw}: conversion must not submit Return"
        );
    }
    for select in [false, true] {
        engines.reset("ja", Mode::Native).unwrap();
        engines
            .command("ja", Mode::Native, &Command::Text("kanji".into()))
            .unwrap();
        let first = engines
            .command("ja", Mode::Native, &Command::Space)
            .unwrap();
        assert_eq!(first.segments, 1);
        assert_eq!(first.selected, Some(0));
        assert!(first.has_next);
        let second = engines
            .command("ja", Mode::Native, &Command::Page(1))
            .unwrap();
        assert_eq!(second.selected, None);
        assert_eq!(second.preedit, first.preedit);
        let expected = if select {
            let chosen = engines
                .command("ja", Mode::Native, &Command::Select(0))
                .unwrap();
            assert_eq!(chosen.selected, Some(0));
            assert_eq!(chosen.preedit, second.candidates[0]);
            let previous = engines
                .command("ja", Mode::Native, &Command::Page(-1))
                .unwrap();
            assert_eq!(previous.selected, None);
            let restored = engines
                .command("ja", Mode::Native, &Command::Page(1))
                .unwrap();
            assert_eq!(restored.selected, Some(0));
            chosen.preedit
        } else {
            first.preedit
        };
        let finished = engines
            .command("ja", Mode::Native, &Command::Enter)
            .unwrap();
        assert_eq!(finished.committed, expected);
        assert_eq!(finished.control, None);
    }
    for cancel in [Command::Escape, Command::Backspace] {
        engines.reset("ja", Mode::Native).unwrap();
        engines
            .command("ja", Mode::Native, &Command::Text("kanji".into()))
            .unwrap();
        engines
            .command("ja", Mode::Native, &Command::Space)
            .unwrap();
        let restored = engines.command("ja", Mode::Native, &cancel).unwrap();
        assert_eq!(restored.preedit, "かんじ");
        let finished = engines
            .command("ja", Mode::Native, &Command::Enter)
            .unwrap();
        assert_eq!(finished.committed, "かんじ");
        assert_eq!(finished.control, None);
    }
    for language in ["ja", "ko", "zh"] {
        engines.reset(language, Mode::Native).unwrap();
        let before = engines
            .command(language, Mode::Native, &Command::Text("ka".into()))
            .unwrap();
        let error = engines
            .command(
                language,
                Mode::Native,
                &Command::Text("a".repeat(MAX_INPUT + 1)),
            )
            .unwrap_err();
        assert!(EngineError::from(error).recoverable);
        let after = engines
            .command(language, Mode::Native, &Command::Finish)
            .unwrap();
        assert!(
            !after.committed.is_empty(),
            "{language}: rejected overflow must retain {:?}",
            before.preedit
        );
    }
    for (raw, reading) in [
        ("gakkou", "がっこう"),
        ("kan'i", "かんい"),
        ("shin'you", "しんよう"),
        ("kyoushitsu", "きょうしつ"),
        ("n", "ん"),
        ("nn", "ん"),
    ] {
        engines.reset("ja", Mode::Native).unwrap();
        engines
            .command("ja", Mode::Native, &Command::Text(raw.into()))
            .unwrap();
        assert_eq!(
            engines
                .command("ja", Mode::Native, &Command::Finish)
                .unwrap()
                .committed,
            reading
        );
    }
    for (raw, reading) in [
        ("shinbun", "しんぶん"),
        ("shinnnyuu", "しんにゅう"),
        ("konnnichiha", "こんにちは"),
        ("k", "k"),
        ("ky", "ky"),
    ] {
        engines.reset("ja", Mode::Native).unwrap();
        engines
            .command("ja", Mode::Native, &Command::Text(raw.into()))
            .unwrap();
        assert_eq!(
            engines
                .command("ja", Mode::Native, &Command::Finish)
                .unwrap()
                .committed,
            reading
        );
    }
    engines.reset("ja", Mode::Katakana).unwrap();
    engines
        .command("ja", Mode::Katakana, &Command::Text("ko-hi-".into()))
        .unwrap();
    assert_eq!(
        engines
            .command("ja", Mode::Katakana, &Command::Finish)
            .unwrap()
            .committed,
        "コーヒー"
    );
    engines.reset("ja", Mode::Native).unwrap();
    engines
        .command("ja", Mode::Native, &Command::Text("kya".into()))
        .unwrap();
    for _ in 0..2 {
        let snapshot = engines.command("ja", Mode::Native, &Command::Left).unwrap();
        assert!(snapshot.preedit.is_char_boundary(snapshot.cursor));
    }
    engines.reset("ja", Mode::Native).unwrap();
    engines
        .command("ja", Mode::Native, &Command::Text("nihongo".into()))
        .unwrap();
    engines
        .command("ja", Mode::Native, &Command::Space)
        .unwrap();
    engines
        .command("ja", Mode::Native, &Command::Escape)
        .unwrap();
    assert!(
        engines
            .command("ja", Mode::Native, &Command::Select(0))
            .is_err()
    );
    engines.reset("ja", Mode::Native).unwrap();
    engines
        .command("ja", Mode::Native, &Command::Text("kya".into()))
        .unwrap();
    let left = engines.command("ja", Mode::Native, &Command::Left).unwrap();
    assert_eq!((&left.preedit, left.cursor), (&"きゃ".to_owned(), 3));
    let inserted = engines
        .command("ja", Mode::Native, &Command::Text("a".into()))
        .unwrap();
    assert_eq!(
        (&inserted.preedit, inserted.cursor),
        (&"きあゃ".to_owned(), 6)
    );
    assert_eq!(
        engines
            .command("ja", Mode::Native, &Command::Backspace)
            .unwrap()
            .preedit,
        "きゃ"
    );
    engines.reset("ja", Mode::Native).unwrap();
    engines
        .command("ja", Mode::Native, &Command::Text("kya".into()))
        .unwrap();
    for expected in ["き", ""] {
        assert_eq!(
            engines
                .command("ja", Mode::Native, &Command::Backspace)
                .unwrap()
                .preedit,
            expected
        );
    }
    engines
        .command("ja", Mode::Native, &Command::Text("shin".into()))
        .unwrap();
    let guarded = engines.command("ja", Mode::Native, &Command::Left).unwrap();
    assert_eq!(guarded.preedit, "しn");
    assert!(guarded.pending_romaji);
    for (raw, expected) in [
        ("gksrmf", "한글"),
        ("dkssudgktpdy", "안녕하세요"),
        ("rkrk", "가가"),
        ("rkrtk", "각사"),
        ("rhk", "과"),
        ("Rk", "까"),
    ] {
        engines.reset("ko", Mode::Native).unwrap();
        assert_eq!(
            engines
                .command("ko", Mode::Native, &Command::Text(raw.into()))
                .unwrap()
                .preedit,
            expected
        );
        assert_eq!(
            engines
                .command("ko", Mode::Native, &Command::Finish)
                .unwrap()
                .committed,
            expected
        );
    }
    engines.reset("ko", Mode::Native).unwrap();
    engines
        .command("ko", Mode::Native, &Command::Text("rkr".into()))
        .unwrap();
    for expected in ["가", "ㄱ", ""] {
        assert_eq!(
            engines
                .command("ko", Mode::Native, &Command::Backspace)
                .unwrap()
                .preedit,
            expected
        );
    }
    for confirm in [Command::Enter, Command::Space, Command::Select(0)] {
        engines.reset("ko", Mode::Native).unwrap();
        let reading = engines
            .command("ko", Mode::Native, &Command::Text("gkswk".into()))
            .unwrap();
        let menu = engines
            .command("ko", Mode::Native, &Command::Hanja)
            .unwrap();
        assert!(!menu.candidates.is_empty());
        assert_eq!(menu.selected, None);
        let expected = match confirm {
            Command::Select(_) => menu.candidates[0].clone(),
            Command::Space => format!("{} ", reading.preedit),
            _ => reading.preedit,
        };
        assert_eq!(
            engines
                .command("ko", Mode::Native, &confirm)
                .unwrap()
                .committed,
            expected
        );
    }
    for (raw, expected) in [("gksrnr", "韓國"), ("gkswk", "漢字")] {
        engines.reset("ko", Mode::Native).unwrap();
        engines
            .command("ko", Mode::Native, &Command::Text(raw.into()))
            .unwrap();
        let mut found = false;
        loop {
            let menu = engines
                .command(
                    "ko",
                    Mode::Native,
                    &if found {
                        Command::Page(1)
                    } else {
                        Command::Hanja
                    },
                )
                .unwrap();
            if menu
                .candidates
                .iter()
                .any(|candidate| candidate == expected)
            {
                break;
            }
            assert!(menu.has_next, "missing {expected}: {menu:?}");
            found = true;
        }
    }
    engines.reset("zh", Mode::Native).unwrap();
    for (mode, raw, expected) in [
        (Mode::Native, "nihao", "你好"),
        (Mode::Native, "zhongguo", "中国"),
        (Mode::Traditional, "zhongguo", "中國"),
    ] {
        engines.reset("zh", mode).unwrap();
        let menu = engines
            .command("zh", mode, &Command::Text(raw.into()))
            .unwrap();
        assert_eq!(menu.candidates.first().map(String::as_str), Some(expected));
        assert_eq!(
            engines
                .command("zh", mode, &Command::Space)
                .unwrap()
                .committed,
            expected
        );
    }
    engines.reset("ja", Mode::Native).unwrap();
    engines
        .command(
            "ja",
            Mode::Native,
            &Command::Text("watashihanihongowobenkyoushiteimasu".into()),
        )
        .unwrap();
    let sentence = engines
        .command("ja", Mode::Native, &Command::Space)
        .unwrap();
    assert!(sentence.segments > 1);
    engines
        .command("ja", Mode::Native, &Command::Segment(1))
        .unwrap();
    engines
        .command("ja", Mode::Native, &Command::Resize(1))
        .unwrap();
    engines
        .command("ja", Mode::Native, &Command::Resize(-1))
        .unwrap();
    let selected = engines
        .command("ja", Mode::Native, &Command::Select(0))
        .unwrap();
    assert!(selected.selection.is_some());
    assert_eq!(
        engines
            .command("ja", Mode::Native, &Command::Enter)
            .unwrap()
            .committed,
        selected.preedit
    );
    engines.reset("zh", Mode::Native).unwrap();
    engines
        .command("zh", Mode::Native, &Command::Text("nihao".into()))
        .unwrap();
    assert_eq!(
        engines
            .command("zh", Mode::Native, &Command::Text("，".into()))
            .unwrap()
            .committed,
        "你好，"
    );
    engines.reset("zh", Mode::Native).unwrap();
    let sentence = engines
        .command(
            "zh",
            Mode::Native,
            &Command::Text("woxiangqubeijing".into()),
        )
        .unwrap();
    assert_eq!(sentence.candidates[0], "我想去北京");
    let prefix = sentence
        .candidates
        .iter()
        .position(|candidate| candidate == "我想去")
        .unwrap();
    let prefix = engines
        .command("zh", Mode::Native, &Command::Select(prefix))
        .unwrap();
    assert!(!prefix.preedit.is_empty());
    let final_text = engines
        .command("zh", Mode::Native, &Command::Finish)
        .unwrap();
    assert_eq!(
        format!("{}{}", prefix.committed, final_text.committed),
        "我想去北京"
    );
    engines.reset("zh", Mode::Native).unwrap();
    engines
        .command("zh", Mode::Native, &Command::Text("shi".into()))
        .unwrap();
    assert_eq!(
        engines
            .command("zh", Mode::Native, &Command::Page(1))
            .unwrap()
            .page,
        1
    );
    assert_eq!(
        engines
            .command("zh", Mode::Native, &Command::Page(-1))
            .unwrap()
            .page,
        0
    );
    for command in [Command::Page(1), Command::Page(-1), Command::Reset] {
        let snapshot = engines.command("zh", Mode::Native, &command).unwrap();
        if snapshot.candidates.is_empty() {
            assert_eq!(snapshot.selected, None);
        } else {
            assert!(
                snapshot
                    .selected
                    .is_some_and(|index| index < snapshot.candidates.len())
            );
        }
    }
    for raw in ["xian", "xi'an", "lv", "lve", "lue", "nv", "nve", "nue"] {
        engines.reset("zh", Mode::Native).unwrap();
        let snapshot = engines
            .command("zh", Mode::Native, &Command::Text(raw.into()))
            .unwrap();
        assert!(!snapshot.candidates.is_empty(), "{raw}");
    }
    fn files(path: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(files(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
    engines.clear();
    for path in files(engines.paths.session.path()) {
        let name = path.to_string_lossy();
        assert!(
            !name.contains("userdb") && !name.contains("history"),
            "Unexpected private input history: {name}"
        );
        if path.starts_with(engines.paths.session.path().join("anthy")) {
            assert_eq!(
                std::fs::metadata(&path).unwrap().len(),
                0,
                "Anthy wrote history: {name}"
            );
        } else if !path.starts_with(&engines.paths.cache) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for typed in ["nihao", "zhongguo", "nihongo", "한글"] {
                assert!(!text.contains(typed), "Typed input persisted in {name}");
            }
        }
    }
    let isolated = tempfile::tempdir().unwrap();
    let runtime = isolated.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "ime::tests::worker_shutdown_helper",
            "--nocapture",
        ])
        .env("NOVAKEYS_SHUTDOWN_TEST", "1")
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("XDG_CACHE_HOME", isolated.path().join("cache"))
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
#[ignore = "private subprocess entry point"]
fn dictionary_deployment_helper() {
    if std::env::var_os("NOVAKEYS_TEST_DEPLOY_USER").is_some() {
        deployment::test_compile().unwrap();
    }
}

#[test]
#[ignore = "private subprocess entry point"]
fn worker_shutdown_helper() {
    if std::env::var_os("NOVAKEYS_SHUTDOWN_TEST").is_none() {
        return;
    }
    fn children() -> Vec<u32> {
        std::fs::read_dir("/proc/self/task")
            .unwrap()
            .flat_map(|entry| {
                let path = entry.unwrap().path().join("children");
                std::fs::read_to_string(path)
                    .unwrap_or_default()
                    .split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect::<Vec<_>>()
            })
            .collect()
    }
    let worker = Worker::new(|| {}).unwrap();
    let token = Token {
        generation: 1,
        epoch: 1,
    };
    worker.reset_context(token);
    worker
        .send(Job {
            token,
            revision: 1,
            language: "zh".into(),
            mode: Mode::Native,
            command: Command::Reset,
        })
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while children().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!children().is_empty(), "Dictionary helper never started");
    let started = std::time::Instant::now();
    drop(worker);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(children().is_empty());
    let runtime = crate::ipc::runtime_dir().unwrap();
    assert!(!std::fs::read_dir(runtime).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("ime-")
    }));
}
