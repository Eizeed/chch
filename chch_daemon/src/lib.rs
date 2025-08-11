use clap::Subcommand;
use std::{
    io::{self, Read, Write, stderr, stdout},
    os::{
        fd::AsFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::spawn_blocking, time::Instant};

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

    #[command(name = "start")]
    Start,

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
            "start" => Ok(ActionWithServer::Start),
            "exit" => Ok(ActionWithServer::Exit),
            _ => Err("Invalid actinon".to_string()),
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
            Self::Start => (DaemonCommand::Start, None),
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
            Self::Start => "start",
            Self::Exit => "exit",
        }
    }
}

#[derive(Debug)]
pub enum DaemonCommand {
    Noop,
    GetState,
    Start,
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
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<DaemonCommand>();

    _ = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(tokio_loop(rx))
    });

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
        if matches!(daemon_command, DaemonCommand::Exit) {
            break;
        }
        tx.send(daemon_command).unwrap();

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

async fn tokio_loop(mut rx: UnboundedReceiver<DaemonCommand>) {
    let mut screen_shotter = ScreenShotter::new(30);

    loop {
        select! {
            command = rx.recv() => {
                match command.unwrap() {
                    DaemonCommand::Start => {
                        screen_shotter.ticking = true;
                        screen_shotter.last = Instant::now();
                    },
                    // DaemonCommand::Stop => {},
                    // DaemonCommand::Reset => {},
                    DaemonCommand::Exit => { break },
                    _ => {}
                }
            },
            _ = tokio::time::sleep(Duration::from_millis(1000)) => {
                if !screen_shotter.ticking {
                    continue
                }

                screen_shotter.ticks += 1;
                if screen_shotter.ticks >= screen_shotter.base_ticks {
                    screen_shotter.ticks = 0;
                    screen_shotter.make_screenshot().await;
                }
            }
        }
    }
}

struct ScreenShotter {
    base_ticks: u16,
    ticks: u16,
    ticking: bool,
    last: Instant,
    last_id: Option<u64>,
}

impl ScreenShotter {
    fn new(base_ticks: u16) -> Self {
        ScreenShotter {
            base_ticks,
            ticks: base_ticks,
            ticking: false,
            last: Instant::now(),
            last_id: None,
        }
    }

    async fn make_screenshot(&mut self) {
        println!("Elapsed: {:.4}", self.last.elapsed().as_secs_f64());
        self.last = Instant::now();

        let id = match self.last_id {
            Some(id) => {
                let new_id = id + 1;
                self.last_id = Some(new_id);

                new_id
            }
            None => spawn_blocking(move || {
                let dir_entries = std::fs::read_dir("/home/lf/timetracking").unwrap();
                let mut ids = dir_entries
                    .map(|d| {
                        let d = d.unwrap();
                        let path = d.path();
                        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy()) else {
                            return 0;
                        };
                        let Some(id_str) = stem.split("_").last() else {
                            return 0;
                        };
                        let Ok(id) = id_str.parse::<u64>() else {
                            return 0;
                        };

                        id
                    })
                    .collect::<Vec<u64>>();

                if ids.len() > 0 {
                    ids.sort();
                    let id = ids[ids.len() - 1];

                    id + 1
                } else {
                    0
                }
            })
            .await
            .unwrap(),
        };

        tokio::process::Command::new("grim")
            .arg("-t")
            .arg("jpeg")
            .arg(format!("/home/lf/timetracking/screen_{}.jpeg", id))
            .spawn()
            .unwrap();
    }
}
