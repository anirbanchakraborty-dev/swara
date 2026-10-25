mod arity;
mod commanddata;
mod completion;
mod definition;
mod diagnostics;
mod document;
mod file_uri;
mod hover;
mod index;
mod info;
mod lilypond;
mod line_index;
mod node_text;
mod server;
mod symbols;
#[cfg(test)]
mod testing;

use std::process::ExitCode;

const USAGE: &str = "usage: swara lsp
       swara format [--indent <n>]
       swara info
       swara --version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["lsp"] => server::run(),
        ["info"] => {
            println!("{}", info::info());
            ExitCode::SUCCESS
        }
        ["--version"] => {
            println!("swara {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["--help"] | ["-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
