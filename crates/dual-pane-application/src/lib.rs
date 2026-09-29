//! Workspace state and use cases.
//!
//! [`Workspace::handle`] is a pure reducer: it validates one input, updates
//! the workspace, and returns application outputs and work requests. It never
//! performs I/O, starts threads, sorts listings, or waits.

mod input;
mod output;
mod work_request;
mod workspace;

pub use input::{Command, Event, Input};
pub use output::Output;
pub use work_request::WorkRequest;
pub use workspace::{Transition, Workspace};
