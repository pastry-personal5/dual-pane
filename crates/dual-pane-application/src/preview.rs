use std::sync::Arc;

use dual_pane_domain::{BrowserSide, EntryName, Location, RequestToken, TabId};

/// A single image request, tied to the exact listing and selected entry that
/// supplied its path. The target dimensions are logical content pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    pub browser: BrowserSide,
    pub tab: TabId,
    pub listing: RequestToken,
    pub name: EntryName,
    pub location: Location,
    pub target_width: u32,
    pub target_height: u32,
    pub token: u64,
}

/// A worker result uses premultiplied 32-bit RGBA pixels, tightly packed in
/// row order, so no Qt image object crosses the application boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewResult {
    Image { pixels: Arc<[u8]>, width: u32, height: u32, source_width: u32, source_height: u32, byte_size: u64, modified_unix_seconds: Option<i64> },
    Unsupported,
    Unavailable,
    DecodeFailed,
    Cancelled,
}

/// The Browser's current preview projection. A new selection clears the old
/// image synchronously, before any background result can arrive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewView {
    Collapsed,
    NoSelection,
    MultipleSelection,
    UnsupportedSelection { name: EntryName },
    Loading { name: EntryName },
    Unavailable { name: EntryName },
    DecodeFailed { name: EntryName },
    Image { name: EntryName, pixels: Arc<[u8]>, width: u32, height: u32, source_width: u32, source_height: u32, byte_size: u64, modified_unix_seconds: Option<i64> },
}
