//! App-level commands emitted by UI panels and applied once per frame by `SmorgasprotApp`.

/// Commands queued by panels during frame layout and executed by the top-level app.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Switch the active dataset (clears selection).
    SelectDataset(usize),
    /// Write the standalone HTML report for all datasets.
    ExportHtml,
    /// Dismiss the current status-bar message.
    DismissStatus,
}
