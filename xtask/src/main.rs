mod gen_song;

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask gen-song --bars <n>";

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
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
