use dual_pane_domain::{Location, RequestToken};

/// Outside work the workspace needs. A runtime carries it out and submits the
/// result as an [`Event`](crate::Event) carrying the same token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkRequest {
    /// Read the directory at `location` and return its entries in listing
    /// order.
    ReadDirectory { token: RequestToken, location: Location },
    /// The request with `token` is no longer needed.
    Cancel { token: RequestToken },
}
