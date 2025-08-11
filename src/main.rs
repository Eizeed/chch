use std::{
    error::Error as StdError,
    fmt::Display,
    os::unix::net::UnixStream,
};

use clap::Parser;

use crate::{
    opts::Opts,
    server::{exit, get_ipc_socket_file, ping, start_daemon},
};

mod ipc_server;
mod opts;
mod server;

fn main() -> Result<(), Error> {
    let opts = Opts::parse();

    match opts.action {
        opts::Action::Deamon => start_daemon(),
        opts::Action::WithServer(action) => match action {
            opts::ActionWithServer::Ping => {
                let mut stream = UnixStream::connect(get_ipc_socket_file()).unwrap();
                let res = ping(&mut stream).unwrap();
                println!("{}", res)
            }
            opts::ActionWithServer::Poll => {}
            opts::ActionWithServer::Exit => {
                let mut stream = UnixStream::connect(get_ipc_socket_file()).unwrap();
                exit(&mut stream).unwrap();
            }
        },
    };

    Ok(())
}

#[derive(Debug)]
pub struct Error {
    message: &'static str,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for Error {}

impl Error {
    pub fn new(message: &str) -> Self {
        Error {
            message: message.to_string().leak(),
        }
    }
}
