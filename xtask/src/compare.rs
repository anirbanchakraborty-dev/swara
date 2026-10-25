//! `cargo xtask compare`: parse the comparison corpus and the broken files with one grammar,
//! loaded from a shared library at run time, and write what it found and how fast it was.

use crate::corpus;
use crate::dyn_grammar::DynGrammar;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tree_sitter::{Node, Parser, Query, QueryCursor, Range, StreamingIterator, Tree};

/// A second grammar for the text an injection query marks, such as Scheme inside LilyPond.
pub struct Inject {
    pub lib: PathBuf,
    pub symbol: String,
    pub query: PathBuf,
}

pub struct Options {
    pub name: String,
    pub commit: Option<String>,
    pub lib: PathBuf,
    pub symbol: String,
    pub inject: Option<Inject>,
    pub out: PathBuf,
}

/// Whether a grammar's first error lands within one line of where LilyPond reports it.
pub fn caught(first_error_line: Option<u32>, expected: u32) -> bool {
    first_error_line.is_some_and(|line| line.abs_diff(expected) <= 1)
}

/// Runs `parse` once to warm caches, then five more times, and returns the median of the five.
pub fn median_time(mut parse: impl FnMut() -> Duration) -> Duration {
    parse();
    let mut times: [Duration; 5] = std::array::from_fn(|_| parse());
    times.sort();
    times[2]
}

/// The first `ERROR` or `MISSING` node in document order, skipping subtrees with no error.
fn first_error(node: Node) -> Option<Node> {
    if !node.has_error() {
        return None;
    }
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find_map(first_error)
}

/// What one grammar made of one file.
struct Analysis {
    error: bool,
    first_error_line: Option<u32>,
    parse: Duration,
    inject_parse: Duration,
}

struct Injector {
    parser: Parser,
    query: Query,
    content: u32,
}

struct Grammars {
    parser: Parser,
    injector: Option<Injector>,
}

impl Grammars {
    fn parse(&mut self, source: &[u8]) -> Result<Tree, String> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| "the parser gave up".to_string())
    }

    /// Runs the injection query over `tree` and parses each marked range with the second grammar.
    fn parse_injections(&mut self, tree: &Tree, source: &[u8]) -> Result<Vec<Tree>, String> {
        let Some(inj) = &mut self.injector else {
            return Ok(Vec::new());
        };
        let mut ranges: Vec<Range> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&inj.query, tree.root_node(), source);
        while let Some(m) = matches.next() {
            for capture in m.captures().iter().filter(|c| c.index == inj.content) {
                ranges.push(capture.node.range());
            }
        }
        let mut trees = Vec::with_capacity(ranges.len());
        for range in ranges {
            inj.parser
                .set_included_ranges(&[range])
                .map_err(|e| format!("injected range: {e:?}"))?;
            trees.push(
                inj.parser
                    .parse(source, None)
                    .ok_or_else(|| "the injected parser gave up".to_string())?,
            );
        }
        Ok(trees)
    }

    fn analyse(&mut self, source: &[u8]) -> Result<Analysis, String> {
        let tree = self.parse(source)?;
        let injected = self.parse_injections(&tree, source)?;
        let roots = std::iter::once(&tree).chain(&injected).map(Tree::root_node);
        let error = roots.clone().any(|n| n.has_error());
        let first_error_line = roots
            .filter_map(first_error)
            .map(|n| n.start_position().row as u32 + 1)
            .min();
        let mut failed = None;
        let parse = median_time(|| {
            let start = Instant::now();
            if let Err(e) = self.parse(source) {
                failed = Some(e);
            }
            start.elapsed()
        });
        let inject_parse = median_time(|| {
            let start = Instant::now();
            if let Err(e) = self.parse_injections(&tree, source) {
                failed = Some(e);
            }
            start.elapsed()
        });
        match failed {
            Some(e) => Err(e),
            None => Ok(Analysis {
                error,
                first_error_line,
                parse,
                inject_parse,
            }),
        }
    }
}

fn parser_for(grammar: &DynGrammar) -> Result<Parser, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&grammar.language)
        .map_err(|e| e.to_string())?;
    Ok(parser)
}

/// `path` relative to `root`, with forward slashes, for the committed results.
fn shown(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

fn ly_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    corpus::collect(dir, &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    files.sort();
    Ok(files)
}

pub fn run(root: &Path, options: &Options) -> Result<(), String> {
    let primary = DynGrammar::load(&options.lib, &options.symbol)?;
    let injected = match &options.inject {
        None => None,
        Some(inject) => Some((DynGrammar::load(&inject.lib, &inject.symbol)?, inject)),
    };
    let injector = match &injected {
        None => None,
        Some((grammar, inject)) => {
            let text = fs::read_to_string(&inject.query)
                .map_err(|e| format!("{}: {e}", inject.query.display()))?;
            // The query is written against the primary grammar's nodes.
            let query = Query::new(&primary.language, &text)
                .map_err(|e| format!("{}: {e}", inject.query.display()))?;
            let content = query
                .capture_index_for_name("injection.content")
                .ok_or_else(|| {
                    format!("{}: no @injection.content capture", inject.query.display())
                })?;
            Some(Injector {
                parser: parser_for(grammar)?,
                query,
                content,
            })
        }
    };
    let mut grammars = Grammars {
        parser: parser_for(&primary)?,
        injector,
    };

    let accepted = ly_files(&root.join("bench/corpus/accepted"))?;
    if accepted.is_empty() {
        return Err("bench/corpus/accepted holds no files: run `cargo xtask corpus filter`".into());
    }
    let broken = ly_files(&root.join("bench/broken"))?;

    let mut files_json = Vec::new();
    let (mut bytes, mut with_error, mut time) = (0u64, 0usize, Duration::ZERO);
    for (i, path) in accepted.iter().enumerate() {
        let source = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let a = grammars
            .analyse(&source)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        bytes += source.len() as u64;
        with_error += usize::from(a.error);
        time += a.parse + a.inject_parse;
        files_json.push(format!(
            "{{\"path\":{},\"bytes\":{},\"error\":{},\"first_error_line\":{},\"parse_us\":{},\"inject_parse_us\":{}}}",
            corpus::json_str(&shown(root, path)),
            source.len(),
            a.error,
            a.first_error_line.map_or("null".into(), |l| l.to_string()),
            a.parse.as_micros(),
            a.inject_parse.as_micros(),
        ));
        if (i + 1) % 500 == 0 {
            println!("compare: {} of {} files", i + 1, accepted.len());
        }
    }

    let mut broken_json = Vec::new();
    let mut broken_caught = 0;
    for path in &broken {
        let source = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let expected = corpus::parse_expectation(&source)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .line;
        let a = grammars
            .analyse(&source)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let hit = caught(a.first_error_line, expected);
        broken_caught += usize::from(hit);
        broken_json.push(format!(
            "{{\"path\":{},\"expected\":{expected},\"first_error_line\":{},\"caught\":{hit}}}",
            corpus::json_str(&shown(root, path)),
            a.first_error_line.map_or("null".into(), |l| l.to_string()),
        ));
    }

    let bytes_per_ms = bytes as f64 / (time.as_secs_f64() * 1000.0);
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\n\"grammar\":{},\n\"commit\":{},\n\"files\":[\n{}\n],\n\"broken\":[\n{}\n],\n",
        corpus::json_str(&options.name),
        options
            .commit
            .as_deref()
            .map_or("null".into(), corpus::json_str),
        files_json.join(",\n"),
        broken_json.join(",\n"),
    );
    let _ = writeln!(
        out,
        "\"summary\":{{\"accepted_files\":{},\"files_with_error\":{with_error},\"bytes_per_ms\":{bytes_per_ms:.1},\"broken_caught\":{broken_caught},\"broken_total\":{}}}\n}}",
        accepted.len(),
        broken.len(),
    );
    fs::write(&options.out, out).map_err(|e| format!("{}: {e}", options.out.display()))?;
    println!(
        "compare {}: {} of {} files with an error, {bytes_per_ms:.1} bytes/ms, {broken_caught} of {} broken files caught",
        options.name,
        with_error,
        accepted.len(),
        broken.len(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn caught_within_one_line() {
        assert!(caught(Some(12), 12));
        assert!(caught(Some(11), 12));
        assert!(caught(Some(13), 12));
        assert!(!caught(Some(14), 12));
        assert!(!caught(Some(10), 12));
        assert!(!caught(None, 12));
    }

    #[test]
    fn timing_takes_median_of_five_after_warmup() {
        // The warm-up returns the slowest time of all, so a median that counted it would move.
        let times = [900, 50, 10, 40, 20, 30, 7000].map(Duration::from_micros);
        let calls = Cell::new(0);
        let median = median_time(|| {
            let t = times[calls.get()];
            calls.set(calls.get() + 1);
            t
        });
        assert_eq!(median, Duration::from_micros(30));
        assert_eq!(calls.get(), 6, "one warm-up and five timed parses");
    }
}
