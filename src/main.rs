//! Thin entry point: parse argv, validate it into a `Config`, run one build.
//! No logic beyond wiring lives here.

use clap::Parser;

fn main() -> std::process::ExitCode {
    let args = ohimark::Args::parse();

    let config = match args.into_config() {
        Ok(config) => config,
        Err(error) => return report_error(&error),
    };

    match ohimark::build(&config) {
        // R10.5: warnings go to stderr only, never stdout, and never
        // change the exit code.
        Ok(warnings) => {
            warnings.emit();
            std::process::ExitCode::SUCCESS
        }
        Err(error) => report_error(&error),
    }
}

/// R10.4: name the failing stage on stderr and exit non-zero.
fn report_error(error: &ohimark::Error) -> std::process::ExitCode {
    eprintln!("ohimark: error: {error}");

    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }

    std::process::ExitCode::FAILURE
}
