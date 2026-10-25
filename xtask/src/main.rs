mod compare;
mod corpus;
mod dyn_grammar;
mod gen_song;
mod sha256;
mod smf;
mod spike_play;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: cargo xtask gen-song --bars <n>
       cargo xtask spike-play --port <p> [--stop-after <ms>] <file.midi>
       cargo xtask corpus fetch
       cargo xtask corpus filter [--jobs <n>] [--timeout-secs <s>]
       cargo xtask broken verify
       cargo xtask compare --name <n> [--commit <sha>] --lib <dylib> --symbol <sym>
                   [--inject-lib <dylib> --inject-symbol <sym> --inject-query <scm>] --out <json>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["gen-song", "--bars", n] => match n.parse::<u32>() {
            Ok(bars) if bars > 0 => {
                print!("{}", gen_song::generate_song(bars));
                ExitCode::SUCCESS
            }
            _ => {
                eprintln!("--bars takes a whole number above zero, not {n:?}");
                ExitCode::from(2)
            }
        },
        ["spike-play", "--port", p, file] => spike_play(p, None, file),
        ["spike-play", "--port", p, "--stop-after", ms, file] => spike_play(p, Some(ms), file),
        ["corpus", "fetch"] => report(corpus::fetch(&root())),
        ["corpus", "filter", options @ ..] => corpus_filter(options),
        ["broken", "verify"] => report(corpus::broken_verify(&root())),
        ["compare", options @ ..] => compare(options),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn spike_play(port: &str, stop_after: Option<&str>, file: &str) -> ExitCode {
    let Ok(port) = port.parse::<u16>() else {
        eprintln!("--port takes a TCP port number, not {port:?}");
        return ExitCode::from(2);
    };
    let stop_after = match stop_after.map(str::parse::<u64>) {
        None => None,
        Some(Ok(ms)) => Some(Duration::from_millis(ms)),
        Some(Err(_)) => {
            eprintln!("--stop-after takes whole milliseconds");
            return ExitCode::from(2);
        }
    };
    match spike_play::run(port, Path::new(file), stop_after) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root: the folder above this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the repository")
        .to_path_buf()
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn corpus_filter(options: &[&str]) -> ExitCode {
    let mut jobs = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut timeout_secs = 60;
    for pair in options.chunks(2) {
        let parsed = match pair {
            ["--jobs", n] => n.parse().map(|n: usize| jobs = n.max(1)).is_ok(),
            ["--timeout-secs", s] => s.parse().map(|s| timeout_secs = s).is_ok(),
            _ => false,
        };
        if !parsed {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    report(corpus::run_filter(
        &root(),
        jobs,
        Duration::from_secs(timeout_secs),
    ))
}

fn compare(options: &[&str]) -> ExitCode {
    let mut values = std::collections::BTreeMap::new();
    for pair in options.chunks(2) {
        match pair {
            [flag, value] if flag.starts_with("--") => {
                values.insert(&flag[2..], value.to_string());
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let mut take = |key: &str| values.remove(key);
    let (Some(name), Some(lib), Some(symbol), Some(out)) =
        (take("name"), take("lib"), take("symbol"), take("out"))
    else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let commit = take("commit");
    let inject = match (
        take("inject-lib"),
        take("inject-symbol"),
        take("inject-query"),
    ) {
        (None, None, None) => None,
        (Some(lib), Some(symbol), Some(query)) => Some(compare::Inject {
            lib: lib.into(),
            symbol,
            query: query.into(),
        }),
        _ => {
            eprintln!("--inject-lib, --inject-symbol and --inject-query go together\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !values.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let options = compare::Options {
        name,
        commit,
        lib: lib.into(),
        symbol,
        inject,
        out: out.into(),
    };
    report(compare::run(&root(), &options))
}
