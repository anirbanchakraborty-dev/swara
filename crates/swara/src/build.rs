//! The full build, on save or on request, and the preview image. A build writes into a hidden
//! folder beside the source, then each output is renamed into the source's folder. A rename on one
//! filesystem replaces the old file in one step, so a viewer watching the PDF never reads half of
//! one. A preview writes into the server's private work folder and is answered with the image.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lsp_types::Uri;
use serde::Serialize;

use crate::lilypond::run::{self, RunKind, RunSpec};

/// The parameters of the `swara/buildFinished` notification.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildFinished {
    pub uri: Uri,
    pub pdf: Option<String>,
    pub midi: Option<String>,
    /// Whether LilyPond exited with success, which it does not after any error.
    pub success: bool,
    pub duration_ms: u64,
}

pub const FINISHED: &str = "swara/buildFinished";

/// What a build left in the source's folder.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outputs {
    pub pdf: Option<PathBuf>,
    pub midi: Option<PathBuf>,
}

/// The name LilyPond gives a file's output, before each extension.
pub fn stem(file: &Path) -> &std::ffi::OsStr {
    file.file_stem().unwrap_or("swara".as_ref())
}

/// The build of `file`: the file on disk when `stdin` is `None`, and otherwise the given text,
/// as for a file not yet saved. Passing the file itself, rather than its saved text on standard
/// input, is what makes the PDF's point-and-click links name the file.
pub fn build_spec(
    uri: &Uri,
    lilypond: &Path,
    file: &Path,
    stdin: Option<String>,
    version: i32,
    include_paths: &[PathBuf],
) -> RunSpec {
    let dir = file.parent().unwrap_or(Path::new("/"));
    let name = format!(".swara-build-{}-{}", std::process::id(), run::next_number());
    let out_dir = dir.join(name);
    let mut args: Vec<OsString> = vec![lilypond.into(), "-l".into(), "WARNING".into()];
    push_includes(&mut args, dir, include_paths);
    args.push("-o".into());
    args.push(out_dir.join(stem(file)).into());
    args.push(if stdin.is_some() {
        "-".into()
    } else {
        file.into()
    });
    RunSpec {
        kind: RunKind::Build,
        uri: uri.clone(),
        version,
        stdin,
        args,
        out_dir,
    }
}

/// The preview of the open text of `file`, cropped to the music, at `dpi` dots per inch. Fails
/// when the work folder cannot be made.
pub fn preview_spec(
    uri: &Uri,
    lilypond: &Path,
    file: &Path,
    text: &str,
    version: i32,
    include_paths: &[PathBuf],
    dpi: u32,
) -> io::Result<RunSpec> {
    let out_dir = run::work_dir()?.join(format!("preview-{}", run::next_number()));
    let dir = file.parent().unwrap_or(Path::new("/"));
    let mut args: Vec<OsString> = vec![lilypond.into(), "-l".into(), "WARNING".into()];
    args.extend([
        "--png".into(),
        "-dcrop".into(),
        format!("-dresolution={dpi}").into(),
    ]);
    push_includes(&mut args, dir, include_paths);
    args.push("-o".into());
    args.push(out_dir.join(stem(file)).into());
    args.push("-".into());
    Ok(RunSpec {
        kind: RunKind::Preview,
        uri: uri.clone(),
        version,
        stdin: Some(text.to_owned()),
        args,
        out_dir,
    })
}

/// The image a preview writes: LilyPond names the cropped one after the output with `.cropped.png`.
pub fn preview_png(out_dir: &Path, file: &Path) -> PathBuf {
    let mut name = stem(file).to_owned();
    name.push(".cropped.png");
    out_dir.join(name)
}

/// The file's folder, then every include folder, each as an absolute `-I`. LilyPond changes into
/// the output folder, so a relative one would be looked up from there.
fn push_includes(args: &mut Vec<OsString>, dir: &Path, include_paths: &[PathBuf]) {
    args.push("-I".into());
    args.push(dir.into());
    for folder in include_paths {
        args.push("-I".into());
        args.push(folder.into());
    }
}

/// Renames every file in `out_dir` into `dir`, removes `out_dir`, and says which of the moved
/// files are the PDF and the MIDI of `file`. LilyPond names MIDI `.midi` or `.mid`, by platform.
pub fn move_outputs(out_dir: &Path, dir: &Path, file: &Path) -> Outputs {
    let mut moved = Vec::new();
    for entry in fs::read_dir(out_dir).into_iter().flatten().flatten() {
        let target = dir.join(entry.file_name());
        if fs::rename(entry.path(), &target).is_ok() {
            moved.push(target);
        }
    }
    let _ = fs::remove_dir_all(out_dir);
    let named = |ext: &str| {
        let mut name = stem(file).to_owned();
        name.push(ext);
        let path = dir.join(name);
        moved.contains(&path).then_some(path)
    };
    Outputs {
        pdf: named(".pdf"),
        midi: named(".midi").or_else(|| named(".mid")),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::testing::Scratch;

    fn args(spec: &RunSpec) -> Vec<String> {
        spec.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn save_build_runs_on_the_file() {
        let uri = Uri::from_str("file:///m/lesson%201.ly").unwrap();
        let file = Path::new("/m/lesson 1.ly");
        let spec = build_spec(
            &uri,
            Path::new("/bin/ly"),
            file,
            None,
            4,
            &[PathBuf::from("/inc")],
        );
        let out = spec.out_dir.to_string_lossy().into_owned();
        assert!(
            out.starts_with(&format!("/m/.swara-build-{}-", std::process::id())),
            "{out}"
        );
        let expected = ["/bin/ly", "-l", "WARNING", "-I", "/m", "-I", "/inc", "-o"];
        assert_eq!(args(&spec)[..8], expected);
        assert_eq!(
            args(&spec)[8..],
            [format!("{out}/lesson 1"), "/m/lesson 1.ly".into()]
        );
        assert_eq!(
            (spec.kind, spec.version, spec.stdin),
            (RunKind::Build, 4, None)
        );
    }

    #[test]
    fn command_build_reads_the_text() {
        let uri = Uri::from_str("file:///m/lesson.ly").unwrap();
        let text = Some("{ c'4 }".to_owned());
        let spec = build_spec(
            &uri,
            Path::new("/bin/ly"),
            Path::new("/m/lesson.ly"),
            text,
            1,
            &[],
        );
        assert_eq!(args(&spec).last().unwrap(), "-");
        assert_eq!(spec.stdin.as_deref(), Some("{ c'4 }"));
    }

    #[test]
    fn each_build_has_its_own_folder() {
        let uri = Uri::from_str("file:///m/lesson.ly").unwrap();
        let build = || {
            build_spec(
                &uri,
                Path::new("ly"),
                Path::new("/m/lesson.ly"),
                None,
                1,
                &[],
            )
        };
        assert_ne!(build().out_dir, build().out_dir);
    }

    #[test]
    fn preview_is_cropped_at_the_resolution() {
        let uri = Uri::from_str("file:///m/lesson.ly").unwrap();
        let file = Path::new("/m/lesson.ly");
        let spec = preview_spec(&uri, Path::new("ly"), file, "{ c'4 }", 2, &[], 150).unwrap();
        let out = spec.out_dir.to_string_lossy().into_owned();
        let expected = [
            "ly",
            "-l",
            "WARNING",
            "--png",
            "-dcrop",
            "-dresolution=150",
            "-I",
            "/m",
        ];
        assert_eq!(args(&spec)[..8], expected);
        assert_eq!(
            args(&spec)[8..],
            ["-o".into(), format!("{out}/lesson"), "-".to_owned()]
        );
        assert_eq!(spec.stdin.as_deref(), Some("{ c'4 }"));
        assert_eq!(
            preview_png(&spec.out_dir, file),
            spec.out_dir.join("lesson.cropped.png")
        );
    }

    #[test]
    fn outputs_move_beside_the_source() {
        let scratch = Scratch::new("build-move");
        let out = scratch.path().join(".swara-build-1");
        fs::create_dir(&out).unwrap();
        for name in ["lesson.pdf", "lesson.mid", "lesson-1.midi"] {
            fs::write(out.join(name), name).unwrap();
        }
        fs::write(scratch.path().join("lesson.pdf"), "old").unwrap();
        let outputs = move_outputs(&out, scratch.path(), Path::new("lesson.ly"));
        let pdf = scratch.path().join("lesson.pdf");
        let midi = scratch.path().join("lesson.mid");
        assert_eq!(
            outputs,
            Outputs {
                pdf: Some(pdf.clone()),
                midi: Some(midi)
            }
        );
        assert_eq!(fs::read_to_string(pdf).unwrap(), "lesson.pdf");
        assert!(scratch.path().join("lesson-1.midi").is_file());
        assert!(!out.exists());
    }

    #[test]
    fn nothing_written_moves_nothing() {
        let scratch = Scratch::new("build-empty");
        let out = scratch.path().join(".swara-build-2");
        fs::create_dir(&out).unwrap();
        let outputs = move_outputs(&out, scratch.path(), Path::new("lesson.ly"));
        assert_eq!(outputs, Outputs::default());
        assert!(!out.exists());
    }
}
