use chch_daemon::ActionWithServer;
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Opts {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    #[command(name = "deamon", alias = "d")]
    Deamon,

    #[command(flatten)]
    WithServer(ActionWithServer),
}

