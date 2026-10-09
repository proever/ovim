//! Send the latest buffer snapshot without blocking Neovim's RPC reader.

use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

pub struct LiveSyncWorker {
    sender: Sender<Option<Vec<String>>>,
    thread: JoinHandle<()>,
}

impl LiveSyncWorker {
    pub fn new(update: impl Fn(Vec<String>) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            while let Ok(Some(mut lines)) = receiver.recv() {
                // Full snapshots supersede each other. Skip stale snapshots
                // accumulated while the browser was processing an update.
                let mut finished = false;
                for newer in receiver.try_iter() {
                    match newer {
                        Some(newer) => lines = newer,
                        None => {
                            finished = true;
                            break;
                        }
                    }
                }
                update(lines);
                if finished {
                    break;
                }
            }
        });
        Self { sender, thread }
    }

    pub fn sender(&self) -> Sender<Option<Vec<String>>> {
        self.sender.clone()
    }

    /// Drain the final snapshot before the edit session completes.
    pub fn finish(self) {
        let _ = self.sender.send(None);
        if self.thread.join().is_err() {
            log::error!("Browser live sync worker stopped unexpectedly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn skips_stale_snapshots_and_drains_final_update() {
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let resume = Mutex::new(resume_rx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&seen);
        let worker = LiveSyncWorker::new(move |lines| {
            if lines == ["first"] {
                started_tx.send(()).expect("signal start");
                resume.lock().expect("resume lock").recv().expect("resume");
            }
            captured.lock().expect("capture lock").push(lines);
        });
        let sender = worker.sender();
        sender
            .send(Some(vec!["first".into()]))
            .expect("first update");
        started_rx.recv().expect("worker started");
        for i in 0..100 {
            sender
                .send(Some(vec![i.to_string()]))
                .expect("queue update");
        }
        resume_tx.send(()).expect("release worker");
        worker.finish();
        assert_eq!(
            *seen.lock().expect("results"),
            vec![vec!["first".to_string()], vec!["99".to_string()]]
        );
    }
}
