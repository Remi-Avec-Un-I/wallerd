use crate::socket::socket_path;

use super::commands::Command;
use log::{debug, error, info};
use std::fs;
use std::io::{BufRead, BufReader, Result, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;

pub fn is_already_running(name: Option<&str>) -> bool {
    UnixStream::connect(socket_path(name)).is_ok()
}

pub fn create_socket(path: &Path) -> Result<UnixListener> {
    fs::remove_file(path).or_else(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    let listener = UnixListener::bind(path)?;
    debug!("Listening on {:?}", path);
    Ok(listener)
}

pub fn handle_stream(listener: UnixListener, send: impl Fn(Command) -> String + Send + 'static) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let mut reader = BufReader::new(stream);
                    let mut msg = String::new();
                    match reader.read_line(&mut msg) {
                        Ok(_) => {
                            let mut stream = reader.into_inner();
                            let msg = msg.trim();
                            info!("Received: {}", msg);
                            let response = if let Some(cmd) = Command::parse(msg) {
                                send(cmd)
                            } else {
                                "error: unknown command".to_string()
                            };
                            let _ = writeln!(stream, "{response}");
                        }
                        Err(e) => error!("Read error: {}", e),
                    }
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    });
}
