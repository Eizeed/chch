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
use tokio::{
    select,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::spawn_blocking,
    time::Instant,
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

    #[command(name = "start")]
    Start,

    #[command(name = "resume")]
    Resume,

    #[command(name = "pause")]
    Pause,

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
            "resume" => Ok(ActionWithServer::Resume),
            "pause" => Ok(ActionWithServer::Pause),
            "exit" => Ok(ActionWithServer::Exit),
            _ => Err("Invalid actinon".to_string()),
        }
    }
}

impl ActionWithServer {
    pub fn into_daemon_command(&self) -> DaemonCommand {
        match self {
            ActionWithServer::Ping => DaemonCommand::Ping,
            ActionWithServer::Poll => DaemonCommand::GetState,
            ActionWithServer::Start => DaemonCommand::Start,
            ActionWithServer::Resume => DaemonCommand::Resume,
            ActionWithServer::Pause => DaemonCommand::Pause,
            ActionWithServer::Exit => DaemonCommand::Exit,
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
            Self::Resume => "resume",
            Self::Pause => "pause",
            Self::Exit => "exit",
        }
    }
}

#[derive(Debug)]
pub enum DaemonCommand {
    Noop,
    Ping,
    GetState,
    Start,
    Resume,
    Pause,
    Exit,
}

pub enum DaemonResponse {
    Success(String),
    Failure(String),
}

fn redirect_output() {
    // let file = std::fs::OpenOptions::new()
    //     .create(true)
    //     .append(true)
    //     .open("/home/lf/personal/rust/chch/log.txt")
    //     .unwrap();

    let file = std::fs::OpenOptions::new()
        .append(true)
        .open("/dev/pts/6")
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
    let (tokio_tx, tokio_rx) = tokio::sync::mpsc::unbounded_channel::<DaemonCommand>();
    let (main_tx, mut main_rx) = tokio::sync::mpsc::unbounded_channel::<TokioMessage>();

    _ = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(tokio_loop(tokio_rx, main_tx))
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

        let daemon_command = action_with_server.into_daemon_command();
        if matches!(daemon_command, DaemonCommand::Exit) {
            break;
        }
        tokio_tx.send(daemon_command).unwrap();

        let op = main_rx.blocking_recv().expect("Daemon died");
        let msg = op.to_string();

        stream.write_all(&(msg.len() as u32).to_be_bytes()).unwrap();
        stream.write_all(msg.as_bytes()).unwrap();
    }
}

async fn tokio_loop(
    mut rx: UnboundedReceiver<DaemonCommand>,
    main_tx: UnboundedSender<TokioMessage>,
) {
    let mut screen_shotter = ScreenShotter::new(1);
    let mut screenshot_timer = Instant::now();

    loop {
        select! {
            command = rx.recv() => {
                match command.expect("Channel closed") {
                    DaemonCommand::Ping => main_tx.send(TokioMessage::Ping).unwrap(),
                    DaemonCommand::Start => {
                        screen_shotter.ticking = true;
                        screenshot_timer = Instant::now();
                        main_tx.send(TokioMessage::Noop).unwrap();
                    },
                    DaemonCommand::Resume => {
                        screen_shotter.paused = false;
                        main_tx.send(TokioMessage::Noop).unwrap();
                    }
                    DaemonCommand::Pause => {
                        screen_shotter.paused = true;
                        main_tx.send(TokioMessage::Noop).unwrap();
                    }
                    DaemonCommand::GetState => {
                        main_tx.send(TokioMessage::State(screen_shotter.clone())).unwrap();
                    }
                    DaemonCommand::Exit => { break },
                    _ => {
                        main_tx.send(TokioMessage::Noop).unwrap();
                    }
                }
            },
            _ = tokio::time::sleep(Duration::from_millis(1000)) => {
                if !screen_shotter.ticking || screen_shotter.paused {
                    continue
                }

                screen_shotter.ticks += 1;
                if screen_shotter.ticks >= screen_shotter.base_ticks {
                    screen_shotter.ticks = 0;
                    screen_shotter.make_screenshot().await;
                    println!("Elapsed: {:.4}", screenshot_timer.elapsed().as_secs_f64());
                    screenshot_timer = Instant::now();
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ScreenShotter {
    base_ticks: u16,
    ticks: u16,
    ticking: bool,
    last_id: Option<u64>,
    paused: bool,
}

impl ScreenShotter {
    fn new(base_ticks: u16) -> Self {
        ScreenShotter {
            base_ticks,
            ticks: base_ticks,
            ticking: false,
            last_id: None,
            paused: false,
        }
    }

    async fn make_screenshot(&mut self) {
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

enum TokioMessage {
    Ping,
    State(ScreenShotter),
    Noop,
}

impl TokioMessage {
    fn to_string(&self) -> String {
        match self {
            TokioMessage::Ping => "ping".to_string(),
            TokioMessage::State(ss) => {
                format!(
                    "base_ticks: {}\nticks: {}\nticking: {}\nlast_id: {}\npaused: {}",
                    ss.base_ticks,
                    ss.ticks,
                    ss.ticking,
                    ss.last_id.unwrap_or_default(),
                    ss.paused
                )
            }
            TokioMessage::Noop => "".to_string(),
        }
    }
}
