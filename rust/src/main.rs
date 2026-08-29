//! `herdr-annotate`: the native Herdr Annotate Lite runtime.

fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match herdr_annotate::run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            #[allow(clippy::print_stderr, reason = "the command boundary reports failures")]
            {
                eprintln!("{error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
