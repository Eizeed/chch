use std::{error::Error as StdError, fmt::Display, os::unix::net::UnixStream};

use chch_daemon::{check_server_running, get_ipc_socket_file, start_daemon_server};
use clap::Parser;

use crate::opts::Opts;

mod opts;

fn main() -> Result<(), Error> {
    let opts = Opts::parse();

    match opts.action {
        opts::Action::Daemon => {
            if check_server_running(&get_ipc_socket_file()) {
                return Err(Error::new("Daemon already running"));
            } else {
                start_daemon_server();
            }
        }
        opts::Action::WithServer(action) => {
            let mut stream = UnixStream::connect(get_ipc_socket_file()).unwrap();
            let maybe_res = action.handle_action(&mut stream).unwrap();
            println!("{:?}", maybe_res);
        }
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
