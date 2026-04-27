fn runtime_dir() -> String {
    std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }))
}

pub fn socket_dir() -> String {
    format!("{}/wallerd", runtime_dir())
}

pub fn all_instances() -> Vec<(String, String)> {
    let dir = socket_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sock") {
            continue;
        }
        let fname = path.file_name().unwrap().to_string_lossy();
        let label = if fname == "wallerd.sock" {
            "default".to_string()
        } else if let Some(name) = fname
            .strip_prefix("wallerd-")
            .and_then(|s| s.strip_suffix(".sock"))
        {
            name.to_string()
        } else {
            continue;
        };
        out.push((label, path.to_string_lossy().into_owned()));
    }
    out
}

pub fn socket_path(name: Option<&str>) -> String {
    let dir = socket_dir();
    match name {
        Some(n) => format!("{dir}/wallerd-{n}.sock"),
        None => format!("{dir}/wallerd.sock"),
    }
}
