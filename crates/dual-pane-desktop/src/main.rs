mod desktop_bridge;

use std::process::ExitCode;

fn main() -> ExitCode {
    let status = desktop_bridge::run_desktop();

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
