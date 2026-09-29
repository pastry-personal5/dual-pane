#[cxx_qt::bridge]
mod cxx_qt_boundary {}

unsafe extern "C" {
    fn dual_pane_run_desktop() -> i32;
}

pub fn run_desktop() -> i32 {
    // SAFETY: `dual_pane_run_desktop` is defined by this crate's C++ shim, takes no arguments,
    // and owns all Qt widget and application lifetime until it returns an event-loop status.
    unsafe { dual_pane_run_desktop() }
}
