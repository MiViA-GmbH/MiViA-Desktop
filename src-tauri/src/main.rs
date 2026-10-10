#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!("release builds need the custom-protocol feature, build through the Tauri CLI");

mod commands;
mod instance;
mod shell;

use std::process::ExitCode;

fn main() -> ExitCode {
    let hardening = (!cfg!(debug_assertions)).then(mivia_os::harden_process);
    match instance::claim() {
        Ok(instance::Outcome::Primary(primary)) => shell::run(*primary, hardening.as_ref()),
        Ok(instance::Outcome::Forwarded) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}
