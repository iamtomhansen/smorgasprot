//! Background work for expensive loads (parsing, interface mapping, surface meshing).
//! Native builds run each job on a thread; wasm has no threads, so jobs run inline.

use crate::dataset::ViewerDataset;
use std::time::Duration;

/// Result of a finished job, applied to the workspace by the app.
pub enum JobOutput {
    /// Newly opened structures, replacing the workspace, plus how long the load took.
    Opened { datasets: Vec<ViewerDataset>, elapsed: Duration },
}

/// A unit of background work; `Err` carries a user-facing message.
pub type JobResult = Result<JobOutput, String>;

struct Job {
    label: String,
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<JobResult>,
    #[cfg(target_arch = "wasm32")]
    result: Option<JobResult>,
}

/// Jobs in flight; poll once per frame.
#[derive(Default)]
pub struct JobQueue {
    jobs: Vec<Job>,
}

impl JobQueue {
    /// Starts `work` and wakes the UI when it finishes; `label` is shown while it runs.
    pub fn spawn(&mut self, ctx: &egui::Context, label: impl Into<String>, work: impl FnOnce() -> JobResult + Send + 'static) {
        let label = label.into();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                // The receiver may be gone if the app closed; nothing to do then.
                let _ = tx.send(work());
                ctx.request_repaint();
            });
            self.jobs.push(Job { label, rx });
        }
        #[cfg(target_arch = "wasm32")]
        {
            ctx.request_repaint();
            self.jobs.push(Job { label, result: Some(work()) });
        }
    }

    /// Removes and returns every finished job as `(label, result)`.
    pub fn poll(&mut self) -> Vec<(String, JobResult)> {
        let mut done = Vec::new();
        self.jobs.retain_mut(|job| match job.try_take() {
            Some(result) => {
                done.push((job.label.clone(), result));
                false
            }
            None => true,
        });
        done
    }

    /// Labels of jobs still running (for the status bar).
    pub fn running(&self) -> impl Iterator<Item = &str> {
        self.jobs.iter().map(|j| j.label.as_str())
    }

    pub fn is_busy(&self) -> bool {
        !self.jobs.is_empty()
    }
}

impl Job {
    #[cfg(not(target_arch = "wasm32"))]
    fn try_take(&mut self) -> Option<JobResult> {
        use std::sync::mpsc::TryRecvError;
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            // A panicking worker drops its sender without sending.
            Err(TryRecvError::Disconnected) => Some(Err(format!("{} failed unexpectedly", self.label))),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn try_take(&mut self) -> Option<JobResult> {
        self.result.take()
    }
}
