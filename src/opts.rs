use chch_daemon::ActionWithServer;
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Opts {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    #[command(name = "daemon", alias = "d")]
    Daemon,

    #[command(flatten)]
    WithServer(ActionWithServer),
}

