use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod eval;
mod generate;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        Some("generate") => generate::run(
            &sdk_root(),
            arguments[1..].iter().any(|argument| argument == "--check"),
        ),
        Some("score-agent-eval") => eval::run(
            &sdk_root(),
            arguments.get(1).map_or("evals/reference", String::as_str),
        ),
        _ => Err("usage: cargo xtask <generate [--check] | score-agent-eval [answers]>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn sdk_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the SDK root")
        .to_path_buf()
}
