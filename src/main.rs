//! Command-line entry point for `ChronoStamp`.

#![forbid(unsafe_code)]

fn main() {
    let code = chrono_stamp::cli::entry(std::env::args_os().skip(1));
    std::process::exit(code);
}
