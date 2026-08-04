use clap::Command;
use std::sync::LazyLock;
mod command {
    include!(concat!(env!("OUT_DIR"), "/command.rs"));
}
pub fn build_cli() -> Command {
    command::add_commands(
        Command::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .about(env!("CARGO_PKG_DESCRIPTION")),
    )
}
pub static CLI: LazyLock<Command> = LazyLock::new(build_cli);
