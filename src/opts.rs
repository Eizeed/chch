use std::str::FromStr;

use clap::{Parser, Subcommand};

use crate::{
    Error,
    ipc_server::{DaemonCommand, DaemonResponse},
};

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

#[derive(Subcommand)]
pub enum ActionWithServer {
    #[command(name = "ping")]
    Ping,

    #[command(name = "poll")]
    Poll,

    #[command(name = "exit")]
    Exit,
}

impl FromStr for ActionWithServer {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        println!("MESSAGE: {}", s);
        match s {
            "ping" => {
                Ok(ActionWithServer::Ping)
            },
            "poll" => Ok(ActionWithServer::Poll),
            "exit" => Ok(ActionWithServer::Exit),
            _ => Err(Error::new("Invalid actino")),
        }
    }
}

impl ActionWithServer {
    pub fn into_daemon_command(&self) -> (DaemonCommand, Option<DaemonResponse>) {
        match self {
            ActionWithServer::Ping => (
                DaemonCommand::Noop,
                Some(DaemonResponse::Success("Pong".to_string())),
            ),
            Self::Poll => (DaemonCommand::GetState, None),
            Self::Exit => (DaemonCommand::Exit, None),
        }
    }
}
