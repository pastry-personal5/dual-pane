use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use dual_pane_application::{PreviewRequest, PreviewResult};

use crate::native_calls::open_preview_file;
use crate::native_location::path_from_location;
use crate::workspace_bridge::ffi::decode_preview;

const MAXIMUM_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

fn field(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

pub fn read_preview(request: &PreviewRequest, cancelled: &AtomicBool) -> PreviewResult {
    if cancelled.load(Ordering::Relaxed) {
        return PreviewResult::Cancelled;
    }
    let file = match open_preview_file(&path_from_location(&request.location)) {
        Ok(file) => file,
        Err(_) => return PreviewResult::Unavailable,
    };
    let metadata = match file.metadata() {
        Ok(metadata) if metadata.len() <= MAXIMUM_SOURCE_BYTES => metadata,
        Ok(_) => return PreviewResult::DecodeFailed,
        Err(_) => return PreviewResult::Unavailable,
    };
    let width = match i32::try_from(request.target_width) {
        Ok(width) => width,
        Err(_) => return PreviewResult::DecodeFailed,
    };
    let height = match i32::try_from(request.target_height) {
        Ok(height) => height,
        Err(_) => return PreviewResult::DecodeFailed,
    };
    let decoded = decode_preview(file.as_raw_fd(), width, height);
    if cancelled.load(Ordering::Relaxed) {
        return PreviewResult::Cancelled;
    }
    let bytes = decoded.as_slice();
    let Some(status) = field(bytes, 0) else { return PreviewResult::DecodeFailed };
    match status {
        1 => PreviewResult::Unsupported,
        2 => PreviewResult::Unavailable,
        3 => PreviewResult::DecodeFailed,
        0 => {
            let (Some(width), Some(height), Some(source_width), Some(source_height)) = (field(bytes, 4), field(bytes, 8), field(bytes, 12), field(bytes, 16)) else { return PreviewResult::DecodeFailed };
            let Some(length) = usize::try_from(width).ok().and_then(|width| usize::try_from(height).ok().and_then(|height| width.checked_mul(height))).and_then(|pixels| pixels.checked_mul(4)) else { return PreviewResult::DecodeFailed };
            if width == 0 || height == 0 || source_width == 0 || source_height == 0 || bytes.len() != 20 + length {
                return PreviewResult::DecodeFailed;
            }
            let modified_unix_seconds = metadata.modified().ok().and_then(|modified| modified.duration_since(UNIX_EPOCH).ok()).and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok());
            PreviewResult::Image { pixels: Arc::from(&bytes[20..]), width, height, source_width, source_height, byte_size: metadata.len(), modified_unix_seconds }
        }
        _ => PreviewResult::DecodeFailed,
    }
}
