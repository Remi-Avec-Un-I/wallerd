use super::{View, boottime_sleep, is_image};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub struct IntervalView {
    images: Vec<PathBuf>,
    interval_secs: u64,
}

impl IntervalView {
    pub fn new(folder: &Path, interval_secs: u64) -> Self {
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

        log::debug!(
            "interval view: {} images, interval {}s",
            images.len(),
            interval_secs
        );

        Self {
            images,
            interval_secs,
        }
    }
}

impl View for IntervalView {
    fn initial(&self) -> PathBuf {
        let path = self.images[0].clone();
        log::debug!("loading image 1/{}: {:?}", self.images.len(), path);
        path
    }

    fn run(self: Box<Self>, send: Box<dyn Fn(std::path::PathBuf) -> bool + Send + 'static>) {
        thread::spawn(move || {
            let mut index = 1 % self.images.len();
            loop {
                boottime_sleep(Duration::from_secs(self.interval_secs));
                log::debug!(
                    "loading image {}/{}: {:?}",
                    index + 1,
                    self.images.len(),
                    self.images[index]
                );
                if !send(self.images[index].clone()) {
                    break;
                }
                index = (index + 1) % self.images.len();
            }
        });
    }
}
