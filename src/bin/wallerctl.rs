use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use log::debug;
use std::io::Write;
use wallerd::socket::{self, socket_path};

#[derive(Parser)]
#[command(name = "wallerctl", about = "Control a running wallerd instance")]
struct Cli {
    #[arg(
        short,
        long,
        help = "Target a specific wallerd instance, or '*' for all"
    )]
    name: Option<String>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Change the current wallpaper
    Wallpaper {
        #[command(subcommand)]
        action: WallpaperAction,
    },
    /// Switch to a different config profile
    Config {
        /// The profile name from the config file, "default" for the default one
        profile: String,
    },
    /// Query daemon information
    List {
        #[command(subcommand)]
        target: ListTarget,
    },
    /// Change the active shader temporarily (until next restart or profile switch)
    Shader {
        #[command(subcommand)]
        action: ShaderAction,
    },
    /// Stop the daemon
    Kill,
    /// Print shell completions to stdout
    Completions { shell: Shell },
    /// Print the man page to stdout
    Man,
}

#[derive(Subcommand)]
enum WallpaperAction {
    /// Set the wallpaper to a specific image
    Set {
        /// Path to the image file
        path: String,
    },
    /// Pause the wallpaper cycling
    Stop,
    /// Resume the wallpaper cycling
    Continue,
}

#[derive(Subcommand)]
enum ListTarget {
    /// List all running wallerd instances
    Instances,
    /// List all profiles with their configuration (JSON)
    Profiles,
    /// List available shaders (JSON)
    Shaders {
        #[command(subcommand)]
        kind: ShadersKind,
    },
}

#[derive(Subcommand)]
enum ShadersKind {
    /// Constant (background effect) shaders
    Constant,
    /// Transition shaders
    Transition,
}

#[derive(Subcommand)]
enum ShaderAction {
    /// Set the active constant shader
    Constant {
        /// Shader name (from ~/.config/wallerd/shaders/constant/ or /usr/share/wallerd/shaders/constant/)
        name: String,
    },
    /// Set the active transition shader
    Transition {
        /// Shader name (from ~/.config/wallerd/shaders/transition/ or /usr/share/wallerd/shaders/transition/)
        name: String,
    },
}

fn connect(path: String) -> UnixStream {
    let stream = UnixStream::connect(&path).unwrap_or_else(|_| {
        eprintln!("Error, could not find the socket {path}");
        std::process::exit(1);
    });
    debug!("Connected to socket: {path}");
    stream
}

fn send(mut stream: UnixStream, msg: String) -> String {
    writeln!(stream, "{msg}").unwrap();
    let mut resp = String::new();
    BufReader::new(stream).read_line(&mut resp).unwrap();
    debug!("Sent: '{msg}'\nReceive: '{resp}'");
    resp
}

fn build_msg(cmd: &Cmd) -> String {
    match cmd {
        Cmd::Wallpaper { action } => match action {
            WallpaperAction::Set { path } => format!("wallpaper set {path}"),
            WallpaperAction::Stop => "wallpaper stop".to_string(),
            WallpaperAction::Continue => "wallpaper continue".to_string(),
        },
        Cmd::Config { profile } => format!("config {profile}"),
        Cmd::List { target } => match target {
            ListTarget::Instances => unreachable!(),
            ListTarget::Profiles => "list profiles".to_string(),
            ListTarget::Shaders { kind } => match kind {
                ShadersKind::Constant => "list shaders constant".to_string(),
                ShadersKind::Transition => "list shaders transition".to_string(),
            },
        },
        Cmd::Shader { action } => match action {
            ShaderAction::Constant { name } => format!("shader set constant {name}"),
            ShaderAction::Transition { name } => format!("shader set transition {name}"),
        },
        Cmd::Kill => "quit".to_string(),
        Cmd::Completions { .. } | Cmd::Man => unreachable!(),
    }
}

fn process_args(cli: Cli) -> String {
    let msg = build_msg(&cli.command);
    send(connect(socket_path(cli.name.as_deref())), msg)
}

fn broadcast(msg: &str) {
    for (label, path) in socket::all_instances() {
        match UnixStream::connect(&path) {
            Ok(stream) => {
                let resp = send(stream, msg.to_string());
                print!("[{label}] {resp}");
            }
            Err(_) => {}
        }
    }
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    match &cli.command {
        Cmd::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut Cli::command(),
                "wallerctl",
                &mut std::io::stdout(),
            );
            return;
        }
        Cmd::Man => {
            clap_mangen::Man::new(Cli::command())
                .render(&mut std::io::stdout())
                .unwrap();
            return;
        }
        Cmd::List {
            target: ListTarget::Instances,
        } => {
            for (label, _) in socket::all_instances() {
                println!("{label}");
            }
            return;
        }
        _ => {}
    }
    if cli.name.as_deref() == Some("*") {
        broadcast(&build_msg(&cli.command));
        return;
    }
    let resp = process_args(cli);
    print!("{resp}");
}
