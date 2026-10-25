//! The build on save, the build and preview commands, and the warnings only a full build finds.
//! The fake `lilypond` covers process control and the protocol. The tests marked
//! `#[ignore = "runs the real LilyPond"]` run LilyPond itself.

mod client;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::{self, sleep};
use std::time::Duration;

use client::fake::{alive, assert_all_gone, dead_pid, start, wait_until};
use client::{TestClient, uri};
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Uri};
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(5);
const REAL_WAIT: Duration = Duration::from_secs(30);
const FINISHED: &str = "swara/buildFinished";

const LESSON: &str = "\\version \"2.26.0\"
\\score {
  \\new Staff { c'4 d' e' f' | g'1 | a'4 g' f' e' | c'1 }
  \\layout { }
  \\midi { }
}
";

fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/lilypond");
    fs::canonicalize(dir.join(name)).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    client::scratch("lsp_build", name)
}

fn file_uri(path: &Path) -> Uri {
    uri(path.to_str().unwrap())
}

fn real_client() -> TestClient {
    TestClient::spawn(json!({"autoCheck": false, "lilypond": null}))
}

/// Writes `text` to `path`, opens it and saves it, as the editor does on `:w`.
fn open_and_save(client: &mut TestClient, path: &Path, text: &str) -> Uri {
    fs::write(path, text).unwrap();
    let file = file_uri(path);
    client.open(&file, text);
    client.save(&file);
    file
}

fn execute(client: &mut TestClient, command: &str, arguments: Value) -> lsp_server::Response {
    client.request_raw(
        "workspace/executeCommand",
        json!({"command": command, "arguments": arguments}),
    )
}

/// The temporary output folders a build leaves in `dir`, which should be none.
fn build_folders(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".swara-build-"))
        .collect()
}

fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-") && bytes.trim_ascii_end().ends_with(b"%%EOF")
}

fn from_lilypond(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.source.as_deref() == Some("lilypond"))
        .collect()
}

#[test]
fn no_build_on_open() {
    let dir = scratch("open");
    let (mut client, fake) = start(&dir, "write:pdf", true, &[]);
    let path = dir.join("lesson.ly");
    fs::write(&path, LESSON).unwrap();
    client.open(&file_uri(&path), LESSON);
    assert!(
        client
            .notifications(FINISHED, Duration::from_secs(1))
            .is_empty()
    );
    assert!(fake.runs().is_empty(), "{:?}", fake.runs());
}

#[test]
fn ily_save_does_not_build() {
    let dir = scratch("ily");
    let (mut client, fake) = start(&dir, "write:pdf", false, &[]);
    open_and_save(&mut client, &dir.join("part.ily"), "partMusic = { c'4 }\n");
    assert!(
        client
            .notifications(FINISHED, Duration::from_secs(1))
            .is_empty()
    );
    assert!(fake.runs().is_empty(), "{:?}", fake.runs());
}

#[test]
fn save_runs_lilypond_on_the_file() {
    let dir = scratch("save").join("আমার গান");
    fs::create_dir_all(&dir).unwrap();
    let (mut client, fake) = start(&dir, "write:pdf,write:midi", false, &[]);
    let path = dir.join("lesson 1.ly");
    let file = open_and_save(&mut client, &path, LESSON);
    let finished = client.wait_notification(FINISHED, WAIT);
    let pdf = dir.join("lesson 1.pdf");
    let midi = dir.join("lesson 1.midi");
    assert_eq!(finished["uri"], file.as_str());
    assert_eq!(finished["success"], true);
    assert_eq!(finished["pdf"], pdf.to_str().unwrap());
    assert_eq!(finished["midi"], midi.to_str().unwrap());
    assert!(finished["durationMs"].is_u64(), "{finished}");
    assert_eq!(fs::read_to_string(&pdf).unwrap(), "fake pdf\n");
    assert_eq!(fs::read_to_string(&midi).unwrap(), "fake midi\n");
    assert!(build_folders(&dir).is_empty(), "{:?}", build_folders(&dir));
    let runs = fake.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let dir = dir.display();
    assert!(
        runs[0].contains(&format!(" -l WARNING -I {dir} -o {dir}/.swara-build-")),
        "{runs:?}"
    );
    assert!(
        runs[0].ends_with(&format!("/lesson 1 {}", path.display())),
        "{runs:?}"
    );
}

#[test]
fn dead_servers_build_folders_swept() {
    // A server killed during a build leaves its output folder beside the score. The next build
    // there removes it once that server has ended, and leaves a running server's folder alone.
    let dir = scratch("dead-builds");
    let dead = format!(".swara-build-{}-7", dead_pid());
    let running = format!(".swara-build-{}-7", std::process::id());
    fs::create_dir_all(dir.join(&dead).join("lesson")).unwrap();
    fs::create_dir_all(dir.join(&running)).unwrap();
    let (mut client, _fake) = start(&dir, "write:pdf", false, &[]);
    open_and_save(&mut client, &dir.join("lesson.ly"), LESSON);
    client.wait_notification(FINISHED, WAIT);
    assert_eq!(build_folders(&dir), [running]);
}

#[test]
fn midi_named_mid_is_reported() {
    let dir = scratch("mid");
    let (mut client, _fake) = start(&dir, "write:pdf,write:mid", false, &[]);
    open_and_save(&mut client, &dir.join("lesson.ly"), LESSON);
    let finished = client.wait_notification(FINISHED, WAIT);
    assert_eq!(finished["midi"], dir.join("lesson.mid").to_str().unwrap());
}

#[test]
fn second_save_supersedes_first() {
    let dir = scratch("supersede");
    let (mut client, fake) = start(&dir, "sleep:3,write:pdf", false, &[]);
    let file = open_and_save(&mut client, &dir.join("lesson.ly"), LESSON);
    sleep(Duration::from_millis(100));
    client.save(&file);
    assert!(
        wait_until(WAIT, || fake.runs().len() == 2),
        "{:?}",
        fake.runs()
    );
    let first: i32 = fake.runs()[0].split(' ').next().unwrap().parse().unwrap();
    assert!(
        wait_until(Duration::from_secs(1), || !alive(first)),
        "the first build still runs"
    );
    let finished = client.notifications(FINISHED, Duration::from_secs(5));
    assert_eq!(finished.len(), 1, "{finished:?}");
    assert_eq!(finished[0]["success"], true);
    assert_all_gone(&fake, 4);
    assert!(build_folders(&dir).is_empty(), "{:?}", build_folders(&dir));
}

#[test]
fn failed_build_reports_errors() {
    let dir = scratch("errors");
    let path = dir.join("lesson.ly");
    let stderr = dir.join("lesson.stderr");
    let name = path.display();
    let report = format!(
        "{name}:2:12: error: syntax error, unexpected '}}'\n{{ c'4 d' }} \n           }}\nfatal error: failed files: \"{name}\"\n"
    );
    fs::write(&stderr, report).unwrap();
    let mode = format!("stderr:{},write:pdf", stderr.display());
    let (mut client, _fake) = start(&dir, &mode, false, &[]);
    let file = open_and_save(&mut client, &path, "\\version \"2.26.0\"\n{ c'4 d' } }\n");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !from_lilypond(d).is_empty());
    let found = from_lilypond(&diagnostics);
    assert_eq!(found.len(), 1, "{diagnostics:?}");
    assert_eq!(found[0].range.start, Position::new(1, 11));
    assert_eq!(found[0].severity, Some(DiagnosticSeverity::ERROR));
    let finished = client.wait_notification(FINISHED, WAIT);
    assert_eq!(finished["success"], false);
    assert_eq!(finished["pdf"], dir.join("lesson.pdf").to_str().unwrap());
}

#[test]
fn build_reports_on_included_files() {
    let dir = scratch("included");
    let part = dir.join("part.ily");
    fs::write(&part, "partMusic = { c'4 \\fooBar }\n").unwrap();
    let stderr = dir.join("part.stderr");
    let report = format!(
        "{}:1:19: error: unknown escaped string: `\\fooBar'\npartMusic = {{ c'4 \n                  \\fooBar }}\n",
        part.display()
    );
    fs::write(&stderr, report).unwrap();
    let mode = format!("stderr:{}", stderr.display());
    let (mut client, _fake) = start(&dir, &mode, false, &[]);
    let text = "\\version \"2.26.0\"\n\\include \"part.ily\"\n{ \\partMusic }\n";
    open_and_save(&mut client, &dir.join("main.ly"), text);
    let (diagnostics, _) = client.wait_diagnostics(&file_uri(&part), WAIT, |d| !d.is_empty());
    assert_eq!(
        diagnostics[0].range.start,
        Position::new(0, 18),
        "{diagnostics:?}"
    );
}

#[test]
fn edits_during_a_build_move_its_messages() {
    let dir = scratch("moving");
    let path = dir.join("lesson.ly");
    let stderr = dir.join("lesson.stderr");
    let report = format!(
        "{}:2:17: warning: No string for pitch #<Pitch e,, > (given frets ())\n\\new TabStaff {{ \n                e,,4 }}\n",
        path.display()
    );
    fs::write(&stderr, report).unwrap();
    let mode = format!("sleep:1,stderr:{}", stderr.display());
    let (mut client, _fake) = start(&dir, &mode, false, &[]);
    let text = fs::read_to_string(fixture("tab-no-string.ly")).unwrap();
    let file = open_and_save(&mut client, &path, &text);
    // Two lines typed above the note while the build runs.
    let top = Position::new(1, 0);
    client.change(&file, 2, Range::new(top, top), "% one\n% two\n");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !from_lilypond(d).is_empty());
    let found = from_lilypond(&diagnostics);
    assert_eq!(
        found[0].range.start,
        Position::new(3, 16),
        "{diagnostics:?}"
    );
}

#[test]
fn build_command_runs_on_stdin() {
    let dir = scratch("command");
    let (mut client, fake) = start(&dir, "stdin,write:pdf", false, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, LESSON);
    let response = execute(&mut client, "swara.build", json!([file.as_str()]));
    assert_eq!(response.response_result.ok(), Some(Value::Null));
    let finished = client.wait_notification(FINISHED, WAIT);
    assert_eq!(finished["success"], true);
    assert_eq!(finished["pdf"], dir.join("lesson.pdf").to_str().unwrap());
    let runs = fake.runs();
    assert!(runs[0].ends_with("/lesson -"), "{runs:?}");
    let stdin = fs::read_to_string(dir.join("fake.log.stdin")).unwrap();
    assert_eq!(stdin, LESSON);
}

#[test]
fn build_command_needs_an_open_file() {
    let dir = scratch("unopened");
    let (mut client, fake) = start(&dir, "write:pdf", false, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    let response = execute(&mut client, "swara.build", json!([file.as_str()]));
    let error = response.response_result.expect_err("an error reply");
    assert_eq!(error.code, lsp_server::ErrorCode::InvalidParams as i32);
    assert!(fake.runs().is_empty(), "{:?}", fake.runs());
}

#[test]
fn runaway_build_times_out() {
    let dir = scratch("runaway");
    let env = [("SWARA_TEST_BUILD_TIMEOUT_MS", "1000")];
    let (mut client, fake) = start(&dir, "sleep:60", false, &env);
    let file = open_and_save(&mut client, &dir.join("lesson.ly"), LESSON);
    let information = Some(DiagnosticSeverity::INFORMATION);
    let (diagnostics, _) =
        client.wait_diagnostics(&file, WAIT, |d| d.iter().any(|d| d.severity == information));
    let stopped: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == information)
        .collect();
    assert_eq!(
        stopped[0].message,
        "LilyPond build took longer than 1 s and was stopped"
    );
    let finished = client.wait_notification(FINISHED, WAIT);
    assert_eq!(finished["success"], false);
    assert_eq!(finished["pdf"], Value::Null);
    assert_all_gone(&fake, 2);
    assert!(build_folders(&dir).is_empty(), "{:?}", build_folders(&dir));
}

#[test]
fn preview_command_answers_with_png() {
    let dir = scratch("preview");
    let (mut client, fake) = start(&dir, "stdin,write:cropped.png", false, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, LESSON);
    let response = execute(&mut client, "swara.preview", json!([file.as_str(), 100]));
    let png = response.response_result.expect("a result")["png"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(png.ends_with("/lesson.cropped.png"), "{png}");
    assert_eq!(fs::read_to_string(&png).unwrap(), "fake cropped.png\n");
    let runs = fake.runs();
    assert!(
        runs[0].contains(" --png -dcrop -dresolution=100 "),
        "{runs:?}"
    );
    assert!(runs[0].ends_with("/lesson -"), "{runs:?}");
    assert_eq!(
        fs::read_to_string(dir.join("fake.log.stdin")).unwrap(),
        LESSON
    );
}

/// Saves `lesson 1.ly` in `dir` with the real LilyPond and checks what the build left there.
fn assert_real_build_beside_source(dir: &Path) {
    let mut client = real_client();
    let path = dir.join("lesson 1.ly");
    open_and_save(&mut client, &path, LESSON);
    let finished = client.wait_notification(FINISHED, REAL_WAIT);
    assert_eq!(finished["success"], true, "{finished}");
    assert_eq!(finished["pdf"], dir.join("lesson 1.pdf").to_str().unwrap());
    assert!(is_pdf(&fs::read(dir.join("lesson 1.pdf")).unwrap()));
    let midi = finished["midi"].as_str().expect("a MIDI file");
    assert!(fs::read(midi).unwrap().starts_with(b"MThd"));
    assert!(build_folders(dir).is_empty(), "{:?}", build_folders(dir));
}

#[test]
#[ignore = "runs the real LilyPond"]
fn save_builds_next_to_source() {
    assert_real_build_beside_source(&scratch("real-save"));
}

#[test]
#[ignore = "runs the real LilyPond"]
fn bengali_and_spaces_in_path() {
    let dir = scratch("real-bengali").join("আমার গান");
    fs::create_dir_all(&dir).unwrap();
    assert_real_build_beside_source(&dir);
}

#[test]
#[ignore = "runs the real LilyPond"]
fn pdf_replaced_atomically() {
    let dir = scratch("real-atomic");
    let pdf = dir.join("lesson.pdf");
    let mut client = real_client();
    let file = open_and_save(&mut client, &dir.join("lesson.ly"), LESSON);
    client.wait_notification(FINISHED, REAL_WAIT);
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let watcher = thread::spawn(move || {
        let mut reads = 0;
        while stopped.try_recv().is_err() {
            let bytes = fs::read(&pdf).unwrap();
            assert!(
                is_pdf(&bytes),
                "read {} bytes that are not a whole PDF",
                bytes.len()
            );
            reads += 1;
            sleep(Duration::from_millis(5));
        }
        reads
    });
    for _ in 0..10 {
        client.save(&file);
        let finished = client.wait_notification(FINISHED, REAL_WAIT);
        assert_eq!(finished["success"], true, "{finished}");
    }
    stop.send(()).unwrap();
    let reads = watcher.join().expect("the watcher saw a partial PDF");
    assert!(reads > 100, "only {reads} reads");
}

#[test]
#[ignore = "runs the real LilyPond"]
fn engraving_warning_kept_and_moved() {
    let dir = scratch("real-engraving");
    let path = dir.join("tab-no-string.ly");
    let text = fs::read_to_string(fixture("tab-no-string.ly")).unwrap();
    let mut client = real_client();
    let file = open_and_save(&mut client, &path, &text);
    let no_string = |d: &[Diagnostic]| {
        d.iter()
            .any(|d| d.message.starts_with("No string for pitch"))
    };
    let (diagnostics, _) = client.wait_diagnostics(&file, REAL_WAIT, no_string);
    let warning = diagnostics
        .iter()
        .find(|d| d.message.starts_with("No string"))
        .unwrap();
    assert_eq!(warning.range.start, Position::new(1, 16));
    assert_eq!(warning.severity, Some(DiagnosticSeverity::WARNING));
    client.wait_notification(FINISHED, REAL_WAIT);

    let top = Position::new(1, 0);
    client.change(&file, 2, Range::new(top, top), "% one\n% two\n");
    let moved = |d: &[Diagnostic]| {
        d.iter()
            .any(|d| d.message.starts_with("No string") && d.range.start == Position::new(3, 16))
    };
    client.wait_diagnostics(&file, WAIT, moved);

    // `e,,4` becomes `e4`, which the tablature can play.
    client.change(
        &file,
        3,
        Range::new(Position::new(3, 17), Position::new(3, 19)),
        "",
    );
    let fixed = "\\version \"2.26.0\"\n% one\n% two\n\\new TabStaff { e4 }\n";
    fs::write(&path, fixed).unwrap();
    client.save(&file);
    client.wait_diagnostics(&file, REAL_WAIT, |d| !no_string(d));
    let finished = client.wait_notification(FINISHED, REAL_WAIT);
    assert_eq!(finished["success"], true, "{finished}");
}

#[test]
#[ignore = "runs the real LilyPond"]
fn build_command_uses_unsaved_text() {
    let dir = scratch("real-command");
    let mut client = real_client();
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, LESSON);
    let response = execute(&mut client, "swara.build", json!([file.as_str()]));
    assert_eq!(response.response_result.ok(), Some(Value::Null));
    let finished = client.wait_notification(FINISHED, REAL_WAIT);
    assert_eq!(finished["success"], true, "{finished}");
    assert!(!dir.join("lesson.ly").exists());
    assert!(is_pdf(&fs::read(dir.join("lesson.pdf")).unwrap()));
}

#[test]
#[ignore = "runs the real LilyPond"]
fn preview_returns_png() {
    let dir = scratch("real-preview");
    let mut client = real_client();
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, LESSON);
    let response = execute(&mut client, "swara.preview", json!([file.as_str(), 100]));
    let png = response.response_result.expect("a result")["png"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(png.ends_with("/lesson.cropped.png"), "{png}");
    assert!(fs::read(&png).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
}
