use clap::{CommandFactory, Parser};
use std::path::PathBuf;
use wallerd::config::parser::load_config_file;
use wallerd::daemon::app::{App, create_event_loop};
use wallerd::daemon::ipc;
use wallerd::daemon::wallpaper;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short, long)]
    profile: Option<String>,

    #[arg(short, long)]
    name: Option<String>,

    #[arg(
        long,
        value_name = "SHELL",
        exclusive = true,
        help = "Print shell completions and exit"
    )]
    completions: Option<clap_complete::Shell>,

    #[arg(long, exclusive = true, help = "Print the man page and exit")]
    man: bool,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        clap_complete::generate(
            shell,
            &mut Cli::command(),
            "wallerd",
            &mut std::io::stdout(),
        );
        return;
    }
    if cli.man {
        clap_mangen::Man::new(Cli::command())
            .render(&mut std::io::stdout())
            .unwrap();
        return;
    }
    let config = load_config_file(cli.config.as_deref()).expect("Failed to load config");

    if ipc::is_already_running(cli.name.as_deref()) {
        eprintln!(
            "error: wallerd ({}) is already running",
            cli.name.as_deref().unwrap_or("default")
        );
        std::process::exit(1);
    }

    let active_mode = match &cli.profile {
        Some(name) => config.additional.get(name).unwrap_or(&config.default),
        None => &config.default,
    }
    .mode
    .clone();

    match active_mode.as_str() {
        "wallpaper" => wallpaper::run(config, cli.profile, cli.name),
        _ => {
            let profiles = config.additional.keys().cloned().collect();
            let (event_loop, proxy) = create_event_loop(
                config.clone(),
                cli.profile.clone(),
                profiles,
                cli.name.as_deref(),
            );
            let mut app = App::new(config, cli.profile, cli.name, proxy);
            event_loop.run_app(&mut app).unwrap();
        }
    }
}
