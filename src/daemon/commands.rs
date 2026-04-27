use std::path::PathBuf;

pub enum WallpaperCmd {
    Set(PathBuf),
    Stop,
    Continue,
}

#[derive(Clone)]
pub enum ShaderKind {
    Constant,
    Transition,
}

pub enum ListCmd {
    Profiles,
    Shaders(ShaderKind),
}

pub enum Command {
    Wallpaper(WallpaperCmd),
    Config(String),
    List(ListCmd),
    SetShader(ShaderKind, String),
    Quit,
}

impl Command {
    pub fn parse(s: &str) -> Option<Self> {
        let (cmd, rest) = s.split_once(' ').unwrap_or((s, ""));
        match cmd {
            "wallpaper" => {
                let (sub, params) = rest.split_once(' ').unwrap_or((rest, ""));
                match sub {
                    "set" => Some(Command::Wallpaper(WallpaperCmd::Set(params.into()))),
                    "stop" => Some(Command::Wallpaper(WallpaperCmd::Stop)),
                    "continue" => Some(Command::Wallpaper(WallpaperCmd::Continue)),
                    _ => None,
                }
            }
            "config" if !rest.is_empty() => Some(Command::Config(rest.trim().to_string())),
            "list" => {
                let (sub, params) = rest.split_once(' ').unwrap_or((rest, ""));
                match sub.trim() {
                    "profiles" => Some(Command::List(ListCmd::Profiles)),
                    "shaders" => match params.trim() {
                        "constant" => Some(Command::List(ListCmd::Shaders(ShaderKind::Constant))),
                        "transition" => {
                            Some(Command::List(ListCmd::Shaders(ShaderKind::Transition)))
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            "shader" => {
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                if parts.len() == 3 && parts[0] == "set" {
                    let name = parts[2].trim().to_string();
                    match parts[1] {
                        "constant" => Some(Command::SetShader(ShaderKind::Constant, name)),
                        "transition" => Some(Command::SetShader(ShaderKind::Transition, name)),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            "quit" => Some(Command::Quit),
            _ => None,
        }
    }
}
