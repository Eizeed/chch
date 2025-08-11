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
