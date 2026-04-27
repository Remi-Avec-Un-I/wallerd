use super::View;
use std::path::PathBuf;

pub struct StaticView {
    pub path: PathBuf,
}

impl View for StaticView {
    fn initial(&self) -> PathBuf {
        self.path.clone()
    }

    fn run(self: Box<Self>, _send: Box<dyn Fn(std::path::PathBuf) -> bool + Send + 'static>) {}
}
