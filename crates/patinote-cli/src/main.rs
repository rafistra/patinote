//! The `patinote` binary: command-line interface (and `patinote mcp` in
//! step 1.8). It never depends on Iced, wgpu or any graphics crate.

mod app;
mod cli;
mod commands;
mod context;
mod output;

fn main() -> std::process::ExitCode {
    // Exit codes are 0..=5 or 130 (A-03), all of which fit in a byte.
    std::process::ExitCode::from(u8::try_from(app::main()).unwrap_or(1))
}
