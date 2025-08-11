use clap::Subcommand;
use std::{
    io::{self, Read, Write, stderr, stdout},
    os::{
        fd::AsFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    str::FromStr,
};

pub fn start_daemon_server() {
    // IDK why forking twice
    let _ = std::fs::remove_file(get_ipc_socket_file());
    match unsafe { nix::unistd::fork().unwrap() } {
        nix::unistd::ForkResult::Child => {
            nix::unistd::setsid().unwrap();
            match unsafe { nix::unistd::fork().unwrap() } {
                nix::unistd::ForkResult::Parent { .. } => std::process::exit(0),
                nix::unistd::ForkResult::Child => {}
            }
        }
        nix::unistd::ForkResult::Parent { .. } => return,
    }

    redirect_output();
    event_loop();
}

pub fn get_ipc_socket_file() -> PathBuf {
    std::fs::create_dir_all("/run/user/1000/chch/").unwrap();
    PathBuf::from("/run/user/1000/chch/chch.sock")
}

pub fn check_server_running(socket_path: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path) else {
        return false;
    };

    ActionWithServer::Ping.handle_action(&mut stream).is_ok()
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
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        println!("MESSAGE: {}", s);
        match s {
            "ping" => Ok(ActionWithServer::Ping),
            "poll" => Ok(ActionWithServer::Poll),
            "exit" => Ok(ActionWithServer::Exit),
            _ => Err("Invalid actino".to_string()),
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

    pub fn handle_action(&self, stream: &mut UnixStream) -> Result<Option<String>, io::Error> {
        let msg_bytes = self.as_str().as_bytes();

        _ = stream.write_all(&(msg_bytes.len() as u32).to_be_bytes());
        _ = stream.write_all(&msg_bytes);

        let mut len_buf = [0u8; 4];
        _ = stream.read_exact(&mut len_buf);

        let len = u32::from_be_bytes(len_buf);
        if len == 0 {
            return Ok(None);
        }

        let mut msg_buf = vec![0u8; len as usize];
        _ = stream.read_exact(&mut msg_buf);

        Ok(Some(String::from_utf8_lossy(&msg_buf).to_string()))
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Ping => "ping",
            Self::Poll => "poll",
            Self::Exit => "exit",
        }
    }
}

#[derive(Debug)]
pub enum DaemonCommand {
    Noop,
    GetState,
    Exit,
}

pub enum DaemonResponse {
    Success(String),
    Failure(String),
}

fn redirect_output() {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/home/lf/personal/rust/chch/log.txt")
        .unwrap();

    let fd = file.as_fd();

    if nix::unistd::isatty(stdout().as_fd()).unwrap() {
        nix::unistd::dup2_stdout(fd).unwrap();
    }
    if nix::unistd::isatty(stderr().as_fd()).unwrap() {
        nix::unistd::dup2_stderr(fd).unwrap();
    }
}

fn event_loop() {
    let listener = UnixListener::bind(get_ipc_socket_file()).unwrap();
    while let Some(Ok(mut stream)) = listener.incoming().next() {
        let mut len_buf = [0u8; 4];
        let msg_len = match stream.read_exact(&mut len_buf) {
            Ok(_) => u32::from_be_bytes(len_buf),
            Err(e) => {
                eprintln!("Failed to read Message Length: {}", e);
                continue;
            }
        };
        let mut msg_buf: Vec<u8> = vec![0u8; msg_len as usize];
        stream.read_exact(&mut msg_buf).unwrap();

        let msg_str = String::from_utf8(msg_buf).unwrap();

        let action_with_server = match ActionWithServer::from_str(msg_str.trim()) {
            Ok(a) => a,
            Err(err) => {
                let msg = err.to_string();

                stream.write_all(&(msg.len() as u32).to_be_bytes()).unwrap();
                stream.write_all(msg.as_bytes()).unwrap();
                continue;
            }
        };

        let (daemon_command, maybe_daemon_response) = action_with_server.into_daemon_command();
        match daemon_command {
            DaemonCommand::Noop => {}
            DaemonCommand::GetState => {}
            DaemonCommand::Exit => break,
        }

        if let Some(daemon_res) = maybe_daemon_response {
            let msg = match daemon_res {
                DaemonResponse::Success(msg) => msg,
                DaemonResponse::Failure(msg) => msg,
            };

            stream.write_all(&(msg.len() as u32).to_be_bytes()).unwrap();
            stream.write_all(msg.as_bytes()).unwrap();
        }
    }
}
