use std::{
    io::{Read, Write, stderr, stdout},
    os::{
        fd::AsFd,
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use crate::{Error, opts::ActionWithServer};

pub fn start_daemon() {
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

    let listener = std::os::unix::net::UnixListener::bind(get_ipc_socket_file()).unwrap();
    while let Ok((mut stream, _)) = listener.accept() {
        let mut len_buf = [0u8; 4];
        let message_len = match stream.read_exact(&mut len_buf) {
            Ok(_) => u32::from_be_bytes(len_buf),
            Err(e) => {
                eprintln!("Ошибка чтения: {}", e);
                continue;
            }
        };

        let mut msg_buf: Vec<u8> = vec![0u8; message_len as usize];
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
        println!("{:?}", daemon_command);
        match daemon_command {
            crate::ipc_server::DaemonCommand::Noop => {}
            crate::ipc_server::DaemonCommand::GetState => {}
            crate::ipc_server::DaemonCommand::Exit => {
                println!("WHY AM I HERE");
                panic!()
            },
        }

        if let Some(daemon_res) = maybe_daemon_response {
            let msg = match daemon_res {
                crate::ipc_server::DaemonResponse::Success(msg) => msg,
                crate::ipc_server::DaemonResponse::Failure(msg) => msg,
            };

            stream.write_all(&(msg.len() as u32).to_be_bytes()).unwrap();

            stream.write_all(msg.as_bytes()).unwrap();
        }
    }
}

pub fn check_server_running(socket_path: &Path) -> bool {
    let mut stream = UnixStream::connect(socket_path).unwrap();
    let res = ping(&mut stream);

    res.is_ok()
}

pub fn get_ipc_socket_file() -> PathBuf {
    std::fs::create_dir_all("/run/user/1000/chch/").unwrap();
    PathBuf::from("/run/user/1000/chch/chch.sock")
}

pub fn ping(stream: &mut UnixStream) -> Result<String, Error> {
    let ping = b"ping";
    stream
        .write_all(&(ping.len() as u32).to_be_bytes())
        .map_err(|e| Error::new(&e.to_string()))?;

    stream
        .write_all(ping)
        .map_err(|e| Error::new(&e.to_string()))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(100)))
        .unwrap();

    let mut len_buf = [0u8; 4];
    println!("Reading len buf");
    stream.read_exact(&mut len_buf).unwrap();
    println!("Len: {}", u32::from_be_bytes(len_buf));

    let mut msg_buf = vec![0u8; u32::from_be_bytes(len_buf) as usize];
    println!("Reading msg buf");
    stream.read_exact(&mut msg_buf).unwrap();
    println!("msg: {}", String::from_utf8_lossy(&msg_buf));

    Ok(String::from_utf8_lossy(&msg_buf).to_string())
}

pub fn exit(stream: &mut UnixStream) -> Result<(), Error> {
    let ping = b"exit";
    stream
        .write_all(&(ping.len() as u32).to_be_bytes())
        .map_err(|e| Error::new(&e.to_string()))?;

    stream
        .write_all(ping)
        .map_err(|e| Error::new(&e.to_string()))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(100)))
        .unwrap();

    Ok(())
}
