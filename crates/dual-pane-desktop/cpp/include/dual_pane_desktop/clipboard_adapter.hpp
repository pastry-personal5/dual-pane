#ifndef dual_pane_DESKTOP_CLIPBOARD_ADAPTER_HPP
#define dual_pane_DESKTOP_CLIPBOARD_ADAPTER_HPP

#include <QtCore/QByteArray>

namespace dual_pane_desktop {

[[nodiscard]] auto clipboard_generation() -> qint64;
/// 0 unsupported, 1 private, 2 file URLs, 3 both.
[[nodiscard]] auto clipboard_offer() -> int;
/// Binary request/result contracts are owned by `clipboard_native.rs`.
[[nodiscard]] auto clipboard_publish(const QByteArray &request) -> QByteArray;
[[nodiscard]] auto clipboard_read() -> QByteArray;

/// Test seam. Isolation gives the calling thread a unique private pasteboard
/// in place of the general one; the seam functions below do nothing outside it.
void clipboard_isolation_begin();
void clipboard_isolation_end();
/// Kinds: 0 replace with a plain string, 1 replace with one file-URL item per
/// line, 2 replace with an item holding `payload` as the private type, 3 append
/// a plain-string item, 4 replace with the file reference URL of the existing file at `payload`.
[[nodiscard]] auto clipboard_seam_write(int kind, const QByteArray &payload) -> bool;
/// One line per item: private+file, private, file, or other.
[[nodiscard]] auto clipboard_seam_describe() -> QByteArray;
[[nodiscard]] auto clipboard_seam_private_payload() -> QByteArray;

} // namespace dual_pane_desktop

#endif // dual_pane_DESKTOP_CLIPBOARD_ADAPTER_HPP
