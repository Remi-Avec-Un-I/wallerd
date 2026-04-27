use super::{View, boottime_sleep, is_image, seconds_since_midnight};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub struct TimeView {
    pub images: Vec<PathBuf>,
    pub start_index: usize,
    pub interval_secs: u64,
}

impl TimeView {
    pub fn new(folder: &Path) -> Self {
        let mut images: Vec<PathBuf> = fs::read_dir(folder)
            .unwrap_or_else(|e| panic!("Failed to read image folder {:?}: {e}", folder))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| is_image(p))
            .collect();
        images.sort_by_cached_key(|p| {
            (
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
                p.clone(),
            )
        });

        if images.is_empty() {
            panic!("No images found in folder {:?}", folder);
        }

        let count = images.len() as u64;
        let interval_secs = (86400 / count).max(1);
        let start_index = (seconds_since_midnight() / interval_secs) as usize % images.len();

        log::debug!(
            "time view: {} images, interval {}s, starting at index {}/{}",
            count,
            interval_secs,
            start_index,
            count - 1
        );

        Self {
            images,
            start_index,
            interval_secs,
        }
    }

    fn current_index(&self) -> usize {
        (seconds_since_midnight() / self.interval_secs) as usize % self.images.len()
    }
}

impl View for TimeView {
    fn initial(&self) -> PathBuf {
        let path = self.images[self.start_index].clone();
        log::debug!(
            "loading image {}/{}: {:?}",
            self.start_index + 1,
            self.images.len(),
            path
        );
        path
    }

    fn run(self: Box<Self>, send: Box<dyn Fn(std::path::PathBuf) -> bool + Send + 'static>) {
        thread::spawn(move || {
            let first_sleep = self.interval_secs - (seconds_since_midnight() % self.interval_secs);
            log::debug!("time view: first switch in {}s", first_sleep);
            boottime_sleep(Duration::from_secs(first_sleep));

            loop {
                let index = self.current_index();
                log::debug!(
                    "loading image {}/{}: {:?}",
                    index + 1,
                    self.images.len(),
                    self.images[index]
                );
                if !send(self.images[index].clone()) {
                    break;
                }

                let remaining =
                    self.interval_secs - (seconds_since_midnight() % self.interval_secs);
                boottime_sleep(Duration::from_secs(remaining));
            }
        });
    }
}
