mod interval_view;
mod static_view;
mod time_view;

pub use interval_view::IntervalView;
pub use static_view::StaticView;
pub use time_view::TimeView;

use crate::config::parser::Config;
use std::path::{Path, PathBuf};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub trait View: Send + 'static {
    fn initial(&self) -> PathBuf;
    /// Called with the next image path. Returns `false` to stop the view loop.
    fn run(self: Box<Self>, send: Box<dyn Fn(PathBuf) -> bool + Send + 'static>);
}

pub fn build(config: &Config) -> Box<dyn View> {
    match config.view.as_str() {
        "time" => Box::new(TimeView::new(&config.path)),
        "interval" => {
            let secs = config.interval.unwrap_or_else(|| {
                log::warn!("interval view requires `interval` in config, defaulting to 60s.");
                60
            });
            Box::new(IntervalView::new(&config.path, secs))
        }
        other => {
            if other != "static" {
                log::warn!("Unknown view '{other}', defaulting to static.");
            }
            Box::new(StaticView { path: config.path.clone() })
        }
    }
}

pub fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp")
    )
}

pub fn seconds_since_midnight() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        % 86400
}

// Uses CLOCK_BOOTTIME so the timer fires correctly after suspend/resume.
// CLOCK_MONOTONIC (used by thread::sleep) pauses during suspend.
pub fn boottime_sleep(duration: Duration) {
    let fd = unsafe { libc::timerfd_create(libc::CLOCK_BOOTTIME, libc::TFD_CLOEXEC) };
    if fd < 0 {
        thread::sleep(duration);
        return;
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };

    let secs = duration.as_secs() as libc::time_t;
    let nsec = duration.subsec_nanos() as libc::c_long;
    let spec = libc::itimerspec {
        it_interval: libc::timespec { tv_sec: 0, tv_nsec: 0 },
        it_value: libc::timespec {
            tv_sec: secs,
            tv_nsec: if secs == 0 && nsec == 0 { 1 } else { nsec },
        },
    };
    unsafe { libc::timerfd_settime(fd.as_raw_fd(), 0, &spec, std::ptr::null_mut()) };

    let mut pollfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pollfd, 1, -1) };
}
