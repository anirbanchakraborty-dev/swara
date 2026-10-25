//! Reading LilyPond's messages from its standard error.
//!
//! Every run passes `-l WARNING` and asks for English, so standard error holds only messages, in
//! English. A message is one line, `FILE:LINE:COL: SEVERITY: TEXT`, with the location, the
//! severity or both left out in some messages. When there is a column, two context lines follow
//! somewhere below it: the source line up to the column, then the rest of it indented by the
//! column. Any other line that follows belongs to the message, such as Guile's own explanation of
//! an error, or the search path after a missing include.

use std::path::PathBuf;

/// Where a message points: the text LilyPond read on standard input, or a file it included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LyFile {
    Stdin,
    Path(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    ProgrammingError,
}

/// One message. `line` and `col` count from 1, as LilyPond prints them, and `col` counts
/// characters with a tab stop every 8 columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LyMessage {
    pub file: Option<LyFile>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub severity: Severity,
    pub text: String,
}

/// The severity words LilyPond prints, in English because every run asks for it.
const SEVERITIES: [(&str, Severity); 4] = [
    ("programming error", Severity::ProgrammingError),
    ("fatal error", Severity::Error),
    ("warning", Severity::Warning),
    ("error", Severity::Error),
];

/// The last line of a failed run, which repeats what the messages above it said.
const FAILED_FILES: &str = "fatal error: failed files: ";

pub fn parse_stderr(stderr: &str) -> Vec<LyMessage> {
    let lines: Vec<&str> = stderr.lines().collect();
    let mut messages: Vec<LyMessage> = Vec::new();
    // Whether the line after a message still belongs to it.
    let mut open = false;
    // The column of the last message while its context lines are still to come.
    let mut awaiting: Option<u32> = None;
    let mut i = 0;
    while i < lines.len() {
        if let Some(col) = awaiting
            && i + 1 < lines.len()
            && is_context(lines[i], lines[i + 1], col)
        {
            awaiting = None;
            i += 2;
            continue;
        }
        let line = lines[i];
        i += 1;
        if line.starts_with(FAILED_FILES) {
            (open, awaiting) = (false, None);
            continue;
        }
        let Some(header) = Header::parse(line) else {
            if let Some(last) = messages.last_mut().filter(|_| open) {
                last.text.push('\n');
                last.text.push_str(line);
            }
            continue;
        };
        let location = header.location;
        let (file, line, col) = match &location {
            Some(l) => (Some(l.file()), Some(l.line), l.col),
            None => (None, None, None),
        };
        match header.severity {
            Some(severity) => {
                let text = header.text.to_owned();
                messages.push(LyMessage { file, line, col, severity, text });
                awaiting = col;
            }
            // A line with a place but no severity word right after a programming error at the same
            // place is LilyPond saying how it carries on, and joins that message.
            None if messages.last().is_some_and(|last| {
                last.severity == Severity::ProgrammingError
                    && (&last.file, last.line, last.col) == (&file, line, col)
            }) =>
            {
                let last = messages.last_mut().expect("checked above");
                last.text.push('\n');
                last.text.push_str(header.text);
                awaiting = None;
            }
            // Otherwise it is an error from the reader, which names no severity.
            None => {
                let text = header.text.to_owned();
                messages.push(LyMessage { file, line, col, severity: Severity::Error, text });
                awaiting = col;
            }
        }
        open = true;
    }
    for message in &mut messages {
        message.text.truncate(message.text.trim_end().len());
    }
    messages
}

/// The first line of a message, split into its parts.
struct Header<'l> {
    location: Option<Location<'l>>,
    severity: Option<Severity>,
    text: &'l str,
}

struct Location<'l> {
    path: &'l str,
    line: u32,
    col: Option<u32>,
}

impl Location<'_> {
    /// `PATH:LINE:COL` or `PATH:LINE`, with a path of at least one character.
    fn parse(s: &str) -> Option<Location<'_>> {
        let (rest, last) = s.rsplit_once(':')?;
        let last = number(last)?;
        if let Some((path, line)) = rest.rsplit_once(':')
            && let Some(line) = number(line)
            && !path.is_empty()
        {
            return Some(Location { path, line, col: Some(last) });
        }
        (!rest.is_empty()).then_some(Location { path: rest, line: last, col: None })
    }

    fn file(&self) -> LyFile {
        if self.path == "-" { LyFile::Stdin } else { LyFile::Path(PathBuf::from(self.path)) }
    }
}

impl<'l> Header<'l> {
    fn parse(line: &'l str) -> Option<Self> {
        for (word, severity) in SEVERITIES {
            if let Some(text) = line.strip_prefix(word).and_then(|rest| rest.strip_prefix(": ")) {
                return Some(Header { location: None, severity: Some(severity), text });
            }
        }
        // The leftmost `: SEVERITY: ` with a location before it, so that a path can hold colons
        // and spaces and the text can hold anything.
        let mut found: Option<(usize, Header)> = None;
        for (word, severity) in SEVERITIES {
            let separator = format!(": {word}: ");
            for (at, _) in line.match_indices(&separator) {
                if found.as_ref().is_some_and(|(earlier, _)| *earlier < at) {
                    break;
                }
                if let Some(location) = Location::parse(&line[..at]) {
                    let text = &line[at + separator.len()..];
                    let header = Header { location: Some(location), severity: Some(severity), text };
                    found = Some((at, header));
                    break;
                }
            }
        }
        if let Some((_, header)) = found {
            return Some(header);
        }
        // A reader error: a location with a column, then the text.
        line.match_indices(": ").find_map(|(at, _)| {
            let location = Location::parse(&line[..at]).filter(|l| l.col.is_some())?;
            Some(Header { location: Some(location), severity: None, text: &line[at + 2..] })
        })
    }
}

fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Whether `source` and `rest` are the context lines of a message at column `col`: the source line
/// up to that column, then spaces to the column and the rest of the line.
fn is_context(source: &str, rest: &str, col: u32) -> bool {
    let indent = col.saturating_sub(1) as usize;
    let mut column = 1;
    for ch in source.chars() {
        column = if ch == '\t' { (column - 1) / 8 * 8 + 9 } else { column + 1 };
    }
    column - 1 == indent && rest.bytes().take_while(|&b| b == b' ').count() >= indent
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/lilypond/");

    fn fixture(name: &str) -> Vec<LyMessage> {
        let stderr = std::fs::read_to_string(format!("{DIR}{name}.stderr")).unwrap();
        parse_stderr(&stderr)
    }

    fn path(p: &str) -> Option<LyFile> {
        Some(LyFile::Path(PathBuf::from(p)))
    }

    #[track_caller]
    fn check(m: &LyMessage, file: Option<LyFile>, at: (u32, u32), severity: Severity, prefix: &str) {
        assert_eq!(m.file, file, "{m:?}");
        assert_eq!((m.line, m.col), (Some(at.0), Some(at.1)), "{m:?}");
        assert_eq!(m.severity, severity, "{m:?}");
        assert!(m.text.starts_with(prefix), "{m:?}");
    }

    #[test]
    fn parse_error() {
        let m = fixture("parse-error");
        assert_eq!(m.len(), 1, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 12), Severity::Error, "syntax error");
        // The two context lines are consumed, not added to the text.
        assert_eq!(m[0].text, "syntax error, unexpected '}'");
    }

    #[test]
    fn unknown_command() {
        let m = fixture("unknown-command");
        assert_eq!(m.len(), 2, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 7), Severity::Error, "unknown command: `\\fooBar'");
        check(&m[1], Some(LyFile::Stdin), (2, 7), Severity::Error, "string outside of text script");
    }

    #[test]
    fn bar_check() {
        let m = fixture("barcheck");
        assert_eq!(m.len(), 1, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 13), Severity::Warning, "bar check failed at: 3/4");
        assert_eq!(m[0].text, "bar check failed at: 3/4");
    }

    #[test]
    fn no_version() {
        let m = fixture("no-version");
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((&m[0].file, m[0].line, m[0].col), (&Some(LyFile::Stdin), Some(1), None));
        assert_eq!(m[0].severity, Severity::Warning);
        assert!(m[0].text.starts_with("no \\version statement found"), "{m:?}");
        // Without a column there are no context lines, and the rest of the message follows.
        assert!(m[0].text.ends_with("for future compatibility"), "{m:?}");
    }

    #[test]
    fn guile_error() {
        let m = fixture("guile-error");
        assert_eq!(m.len(), 1, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 2), Severity::Error, "Guile signaled an error");
        // Guile's own explanation follows the context lines and belongs to the message.
        assert!(m[0].text.ends_with("\nIn procedure car: Wrong type (expecting pair): ()"), "{m:?}");
        assert!(!m[0].text.contains("(car '())"), "{m:?}");
    }

    #[test]
    fn reader_error() {
        let m = fixture("reader-error");
        assert_eq!(m.len(), 4, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 2), Severity::Error, "Guile signaled an error");
        // No severity word, so an error. Nothing follows it but the next message.
        check(&m[1], Some(LyFile::Stdin), (2, 13), Severity::Error, "Unknown # object: \"#<\"");
        assert_eq!(m[1].text, "Unknown # object: \"#<\"");
        check(&m[2], Some(LyFile::Stdin), (2, 2), Severity::Error, "syntax error");
        check(&m[3], Some(LyFile::Stdin), (2, 13), Severity::Error, "Guile signaled an error");
        assert!(m[3].text.ends_with("\nUnbound variable: <foo>"), "{m:?}");
    }

    #[test]
    fn no_location() {
        let m = fixture("no-location");
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((&m[0].file, m[0].line, m[0].col), (&None, None, None));
        assert_eq!(m[0].severity, Severity::Warning);
        assert_eq!(m[0].text, "no such program option: such-option");
    }

    #[test]
    fn include_error() {
        let m = fixture("include-error");
        assert_eq!(m.len(), 1, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (2, 10), Severity::Error, "cannot find file: `missing-part.ily'");
        // The search path comes before the context lines, which are still found and dropped.
        assert!(m[0].text.contains("\n(search path: `/"), "{m:?}");
        assert!(!m[0].text.contains("\\include"), "{m:?}");
    }

    #[test]
    fn stdin_and_included_file() {
        let m = fixture("stdin-include");
        assert_eq!(m.len(), 2, "{m:?}");
        check(&m[0], Some(LyFile::Stdin), (3, 12), Severity::Error, "syntax error");
        let included = path("/fixture/stdin-include.ily");
        check(&m[1], included, (2, 25), Severity::Warning, "bar check failed at: 3/4");
    }

    #[test]
    fn tab_and_bengali_columns() {
        let tab = fixture("tab-column");
        assert_eq!(tab.len(), 1, "{tab:?}");
        check(&tab[0], Some(LyFile::Stdin), (2, 17), Severity::Error, "syntax error");
        assert_eq!(tab[0].text, "syntax error, unexpected '}'");
        let bengali = fixture("utf8-column");
        assert_eq!(bengali.len(), 1, "{bengali:?}");
        check(&bengali[0], Some(LyFile::Stdin), (2, 15), Severity::Error, "syntax error");
        assert_eq!(bengali[0].text, "syntax error, unexpected '}'");
    }

    #[test]
    fn programming_error() {
        let m = fixture("programming-error");
        assert_eq!(m.len(), 1, "{m:?}");
        let file = path("/fixture/programming-error.ly");
        check(&m[0], file, (2, 84), Severity::ProgrammingError, "cyclic dependency");
        // The follow-up line at the same place joins the message instead of becoming an error.
        assert!(m[0].text.ends_with("\ncontinuing, cross fingers"), "{m:?}");
    }

    #[test]
    fn seeded_check() {
        let m = fixture("seeded.check");
        let lines: Vec<_> = m.iter().map(|m| (m.line, m.severity)).collect();
        let (e, w) = (Severity::Error, Severity::Warning);
        assert_eq!(lines, [(Some(3), e), (Some(150), e), (Some(210), e), (Some(20), w)]);
    }

    #[test]
    fn path_with_spaces_and_bengali() {
        let m = parse_stderr("/Users/x/আমার গান/lesson 1.ly:3:5: error: syntax error\n");
        assert_eq!(m.len(), 1, "{m:?}");
        check(&m[0], path("/Users/x/আমার গান/lesson 1.ly"), (3, 5), Severity::Error, "syntax error");
    }

    #[test]
    fn windows_drive_letter() {
        let m = parse_stderr("C:\\music\\a.ly:3:5: warning: bar check failed at: 1/4\r\n");
        check(&m[0], path("C:\\music\\a.ly"), (3, 5), Severity::Warning, "bar check failed");
    }

    #[test]
    fn fatal_summary_ignored() {
        assert_eq!(parse_stderr("fatal error: failed files: \"-\"\n"), []);
    }

    #[test]
    fn separator_inside_the_text() {
        let m = parse_stderr("-:4:2: warning: expected: error: none\n");
        check(&m[0], Some(LyFile::Stdin), (4, 2), Severity::Warning, "expected: error: none");
    }

    #[test]
    fn context_line_that_looks_like_a_message() {
        // The source line itself starts with `error: `, and is still read as context.
        let m = parse_stderr("-:2:8: error: syntax error\nerror: \n       x }\n");
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].text, "syntax error");
    }

    #[test]
    fn crlf_line_ends() {
        let stderr = std::fs::read_to_string(format!("{DIR}unknown-command.stderr")).unwrap();
        assert_eq!(parse_stderr(&stderr.replace('\n', "\r\n")), fixture("unknown-command"));
    }
}
