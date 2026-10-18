//! Binary, byte-exact boundary around the Objective-C++ pasteboard adapter.

use std::sync::Arc;

use cxx_qt_lib::QByteArray;
use dual_pane_application::{ClipboardAction, ClipboardGeneration, ClipboardOffer, ClipboardSnapshot, TransferId};
use dual_pane_domain::{EntryKind, EntryName, Location, OperationRoot};

use crate::workspace_bridge::ffi;

pub fn generation_and_offer() -> (ClipboardGeneration, ClipboardOffer) {
    let generation = ClipboardGeneration::new(u64::try_from(ffi::clipboard_generation()).unwrap_or(0));
    let offer = match ffi::clipboard_offer() {
        1 => ClipboardOffer::Private,
        2 => ClipboardOffer::Files,
        3 => ClipboardOffer::PrivateAndFiles,
        _ => ClipboardOffer::Unsupported,
    };
    (generation, offer)
}

pub fn publish(transfer: TransferId, action: ClipboardAction, roots: Option<&[OperationRoot]>) -> (ClipboardGeneration, bool) {
    let mut request = b"DPCB1".to_vec();
    request.extend_from_slice(&transfer.get().to_le_bytes());
    request.push(u8::from(action == ClipboardAction::Cut));
    let roots = roots.unwrap_or(&[]);
    request.extend_from_slice(&u32::try_from(roots.len()).unwrap_or(u32::MAX).to_le_bytes());
    for root in roots {
        request.push(u8::from(root.kind == EntryKind::Directory));
        let path = path_bytes(&root.source);
        request.extend_from_slice(&u32::try_from(path.len()).unwrap_or(u32::MAX).to_le_bytes());
        request.extend_from_slice(&path);
    }
    let result = ffi::clipboard_publish(&QByteArray::from(request.as_slice())).as_ref().to_vec();
    if result.len() != 9 {
        return (generation_and_offer().0, false);
    }
    let generation = ClipboardGeneration::new(u64::from_le_bytes(result[..8].try_into().unwrap_or_default()));
    (generation, result[8] != 0)
}

pub fn read() -> (ClipboardGeneration, ClipboardSnapshot) {
    let bytes = ffi::clipboard_read().as_ref().to_vec();
    let mut cursor = Cursor::new(&bytes);
    let Some(generation) = cursor.u64() else { return (generation_and_offer().0, ClipboardSnapshot::Unsupported) };
    let Some(flags) = cursor.byte() else { return (ClipboardGeneration::new(generation), ClipboardSnapshot::Unsupported) };
    let transfer = if flags & 1 != 0 { cursor.u64().map(TransferId::new) } else { None };
    let Some(count) = cursor.u32() else { return (ClipboardGeneration::new(generation), ClipboardSnapshot::Unsupported) };
    // Every path carries a four-byte length, so a larger count cannot be honest.
    let mut sources = Vec::with_capacity((count as usize).min(bytes.len() / 4));
    for _ in 0..count {
        let Some(path) = cursor.bytes() else { return (ClipboardGeneration::new(generation), ClipboardSnapshot::Unsupported) };
        let Some(location) = location_from_absolute_bytes(path) else { return (ClipboardGeneration::new(generation), ClipboardSnapshot::Unsupported) };
        sources.push(location);
    }
    if flags & 4 != 0 || !cursor.done() {
        return (ClipboardGeneration::new(generation), ClipboardSnapshot::Unsupported);
    }
    let sources: Arc<[Location]> = sources.into();
    let snapshot = match (transfer, sources.is_empty()) {
        (Some(transfer), false) => ClipboardSnapshot::PrivateAndExternal { transfer, sources },
        (Some(transfer), true) => ClipboardSnapshot::Private { transfer },
        (None, false) => ClipboardSnapshot::ExternalFiles { sources },
        (None, true) => ClipboardSnapshot::Unsupported,
    };
    (ClipboardGeneration::new(generation), snapshot)
}

fn path_bytes(location: &Location) -> Vec<u8> {
    let mut bytes = Vec::new();
    for component in location.components() {
        bytes.push(b'/');
        bytes.extend_from_slice(component.as_bytes());
    }
    if bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes
}

fn location_from_absolute_bytes(path: &[u8]) -> Option<Location> {
    if path.first() != Some(&b'/') || path.contains(&0) {
        return None;
    }
    let components = path[1..].split(|byte| *byte == b'/').filter(|component| !component.is_empty()).map(|component| EntryName::new(component.to_vec())).collect::<Result<Vec<_>, _>>().ok()?;
    Some(Location::from_components(components))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn byte(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.offset)?;
        self.offset += 1;
        Some(value)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.u32()?).ok()?;
        self.take(length)
    }
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(length)?;
        let value = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }
    fn done(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gives the test thread a unique pasteboard, never the user's clipboard.
    struct Isolated;
    impl Isolated {
        fn new() -> Self {
            ffi::clipboard_isolation_begin();
            Self
        }
    }
    impl Drop for Isolated {
        fn drop(&mut self) {
            ffi::clipboard_isolation_end();
        }
    }

    fn root(parts: &[&[u8]], kind: EntryKind) -> OperationRoot {
        OperationRoot { source: Location::from_components(parts.iter().map(|part| EntryName::new(part.to_vec()).unwrap())), kind }
    }
    fn describe() -> String {
        String::from_utf8(ffi::clipboard_seam_describe().as_ref().to_vec()).unwrap()
    }
    fn write(kind: i32, payload: &[u8]) -> bool {
        ffi::clipboard_seam_write(kind, &QByteArray::from(payload))
    }

    #[test]
    fn absolute_paths_round_trip_arbitrary_component_bytes() {
        let location = Location::from_components([EntryName::new(b"one".to_vec()).unwrap(), EntryName::new(b"raw\xff".to_vec()).unwrap()]);
        assert_eq!(location_from_absolute_bytes(&path_bytes(&location)), Some(location));
        assert_eq!(location_from_absolute_bytes(b"relative"), None);
    }

    #[test]
    fn copy_writes_a_file_url_item_per_root_in_order_and_a_private_token_without_paths() {
        let _isolated = Isolated::new();
        let roots = [root(&[b"tmp", b"dp-a", b"folder"], EntryKind::Directory), root(&[b"tmp", b"dp-b", b"raw\xff name"], EntryKind::File), root(&[b"tmp", b"dp-a", "caf\u{e9} and #?%".as_bytes()], EntryKind::File), root(&[b"tmp", b"dp-a", "cafe\u{301}".as_bytes()], EntryKind::File), root(&[b"tmp", b"dp-a", b"lone\x80byte"], EntryKind::File)];
        let (before, _) = generation_and_offer();
        let (generation, succeeded) = publish(TransferId::new(77), ClipboardAction::Copy, Some(&roots));
        assert!(succeeded && generation > before);
        assert_eq!(describe(), "private+file\nfile\nfile\nfile\nfile");
        assert_eq!(generation_and_offer(), (generation, ClipboardOffer::PrivateAndFiles));
        let payload = ffi::clipboard_seam_private_payload().as_ref().to_vec();
        assert_eq!(payload.len(), 9, "version byte and TransferId only");
        assert!(!payload.windows(3).any(|window| window == b"tmp"));

        let (read_generation, snapshot) = read();
        assert_eq!(read_generation, generation);
        let ClipboardSnapshot::PrivateAndExternal { transfer, sources } = snapshot else { panic!("both representations") };
        assert_eq!(transfer, TransferId::new(77));
        assert_eq!(sources.as_ref(), roots.iter().map(|root| root.source.clone()).collect::<Vec<_>>().as_slice(), "byte-exact paths in clipboard order");
    }

    #[test]
    fn cut_writes_one_private_only_item_and_never_file_urls() {
        let _isolated = Isolated::new();
        let (generation, succeeded) = publish(TransferId::new(5), ClipboardAction::Cut, None);
        assert!(succeeded);
        assert_eq!(describe(), "private");
        assert_eq!(generation_and_offer(), (generation, ClipboardOffer::Private));
        assert_eq!(read(), (generation, ClipboardSnapshot::Private { transfer: TransferId::new(5) }));

        let (after, accepted) = publish(TransferId::new(6), ClipboardAction::Cut, Some(&[root(&[b"tmp", b"x"], EntryKind::File)]));
        assert!(!accepted, "a Cut cannot carry file references");
        assert_eq!(after, generation, "a refused publication leaves the clipboard untouched");
        assert_eq!(read(), (generation, ClipboardSnapshot::Private { transfer: TransferId::new(5) }));
    }

    #[test]
    fn finder_style_file_urls_read_in_order_as_external_files() {
        let _isolated = Isolated::new();
        assert!(write(1, b"file:///tmp/dp-a/one\nfile:///tmp/dp-b/two%20words/"));
        assert_eq!(generation_and_offer().1, ClipboardOffer::Files);
        let ClipboardSnapshot::ExternalFiles { sources } = read().1 else { panic!("external files") };
        let expected = [Location::from_components([EntryName::new(b"tmp".to_vec()).unwrap(), EntryName::new(b"dp-a".to_vec()).unwrap(), EntryName::new(b"one".to_vec()).unwrap()]), Location::from_components([EntryName::new(b"tmp".to_vec()).unwrap(), EntryName::new(b"dp-b".to_vec()).unwrap(), EntryName::new(b"two words".to_vec()).unwrap()])];
        assert_eq!(sources.as_ref(), expected.as_slice());
    }

    #[test]
    fn malformed_non_file_and_mixed_content_is_unsupported_without_a_valid_subset() {
        let _isolated = Isolated::new();
        let cases: [(&str, i32, &[u8]); 12] = [("plain text", 0, b"plain text"), ("web address", 1, b"https://example.com/a"), ("unparsable address", 1, b"not a url"), ("empty address", 1, b""), ("short private token", 2, b"short"), ("future private token version", 2, &[2, 1, 0, 0, 0, 0, 0, 0, 0]), ("relative address", 1, b"file:relative"), ("remote host", 1, b"file://server/tmp/a"), ("bad percent escape", 1, b"file:///tmp/a%zz"), ("encoded NUL", 1, b"file:///tmp/a%00b"), ("query", 1, b"file:///tmp/a?x=1"), ("space", 1, b"file:///tmp/a b")];
        for (label, kind, payload) in cases {
            assert!(write(kind, payload), "{label} is written");
            assert_eq!(read().1, ClipboardSnapshot::Unsupported, "{label}");
        }
        assert!(write(1, b"file:///tmp/dp-a/one"));
        assert!(write(3, b"stray text item"));
        assert_eq!(read().1, ClipboardSnapshot::Unsupported, "one bad item rejects the whole snapshot");
    }

    #[test]
    fn generations_advance_on_replacement_and_a_failed_publication_reports_the_current_one() {
        let _isolated = Isolated::new();
        let roots = [root(&[b"tmp", b"dp-a", b"one"], EntryKind::File)];
        let (generation, _) = publish(TransferId::new(1), ClipboardAction::Copy, Some(&roots));
        let (failed, succeeded) = publish(TransferId::new(2), ClipboardAction::Copy, Some(&[]));
        assert!(!succeeded);
        assert_eq!(failed, generation, "nothing was written");
        assert!(matches!(read().1, ClipboardSnapshot::PrivateAndExternal { transfer, .. } if transfer == TransferId::new(1)));

        assert!(write(0, b"another application"));
        let (replaced, offer) = generation_and_offer();
        assert!(replaced > generation);
        assert_eq!(offer, ClipboardOffer::Unsupported);
        assert_eq!(read(), (replaced, ClipboardSnapshot::Unsupported));
    }

    #[test]
    fn a_file_reference_url_resolves_to_the_files_current_path_and_a_dangling_one_is_refused() {
        use std::os::unix::ffi::OsStrExt;

        let _isolated = Isolated::new();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("referenced.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(write(4, file.as_os_str().as_bytes()));
        let ClipboardSnapshot::ExternalFiles { sources } = read().1 else { panic!("a resolved reference reads as an external file") };
        assert_eq!(sources.len(), 1);
        let resolved = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&path_bytes(&sources[0])));
        assert_eq!(std::fs::canonicalize(resolved).unwrap(), std::fs::canonicalize(&file).unwrap());

        std::fs::remove_file(&file).unwrap();
        let snapshot = read().1;
        assert!(!matches!(&snapshot, ClipboardSnapshot::ExternalFiles { sources } if sources.iter().any(|source| path_bytes(source).starts_with(b"/.file/"))), "an unresolved reference is never a path");
    }
}
