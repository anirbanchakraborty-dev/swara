mod gen_song;
mod smf;
mod spike_play;

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: cargo xtask gen-song --bars <n>
       cargo xtask spike-play --port <p> [--stop-after <ms>] <file.midi>";

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
