//! Running LilyPond and reading what it prints.

pub mod messages;
pub mod run;

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::line_index::{Encoding, LineIndex};
use messages::{LyMessage, Severity};

/// A diagnostic for one message about `text`, with UTF-8 positions. It covers the word at the
/// message's column, and is empty at the start of the line when the message has no column, or at
/// the start of the text when it has no line.
pub fn to_diagnostic(message: &LyMessage, text: &str, index: &LineIndex) -> Diagnostic {
    let (start, end) = match (message.line, message.col) {
        (Some(line), Some(col)) => {
            let start = index.lilypond_column_to_byte(text, line.saturating_sub(1), col);
            (start, start + word_len(&text[start..]))
        }
        (Some(line), None) => {
            let start = Position::new(line.saturating_sub(1), 0);
            let start = index.position_to_byte(text, start, Encoding::Utf8);
            (start, start)
        }
        (None, _) => (0, 0),
    };
    let severity = match message.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        // LilyPond's own fault, and the score still engraves.
        Severity::Warning | Severity::ProgrammingError => DiagnosticSeverity::WARNING,
    };
    let position = |byte| index.byte_to_position(text, byte, Encoding::Utf8);
    Diagnostic {
        range: Range::new(position(start), position(end)),
        severity: Some(severity),
        source: Some("lilypond".into()),
        message: message.text.clone(),
        ..Default::default()
    }
}

/// The length of the word that starts `rest`: a command with its backslash, or a note with its
/// octave marks and duration, and otherwise one character. Nothing at the end of a line.
fn word_len(rest: &str) -> usize {
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else { return 0 };
    if first == '\n' || first == '\r' {
        return 0;
    }
    if first != '\\' && !first.is_alphanumeric() {
        return first.len_utf8();
    }
    let in_word = |c: char| c.is_alphanumeric() || c == '\'' || c == ',';
    chars.find(|&(_, c)| !in_word(c)).map_or(rest.len(), |(i, _)| i)
}
