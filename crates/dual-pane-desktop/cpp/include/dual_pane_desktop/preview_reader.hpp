#ifndef DUAL_PANE_DESKTOP_PREVIEW_READER_HPP
#define DUAL_PANE_DESKTOP_PREVIEW_READER_HPP

#include <QtCore/QByteArray>

namespace dual_pane_desktop {

// Returns five little-endian u32 fields (status, displayed pixel width and
// height, oriented source width and height), followed by tight RGBA pixels.
[[nodiscard]] auto decode_preview(int fd, int target_width, int target_height) -> QByteArray;

} // namespace dual_pane_desktop

#endif // DUAL_PANE_DESKTOP_PREVIEW_READER_HPP
