use std::sync::Arc;

use caseless::default_case_fold_str;
use dual_pane_domain::{BrowserSide, Entry, RequestToken, TabId};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Immutable inputs for one tab's name projection. Every field is checked
/// again by the reducer before a worker result may become visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterRequest {
    pub browser: BrowserSide,
    pub tab: TabId,
    pub listing: RequestToken,
    pub visibility_generation: u64,
    pub generation: u64,
    pub text: String,
    pub entries: Arc<[Entry]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterResult {
    pub indices: Arc<[usize]>,
    pub entries: Arc<[Entry]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFailure {
    WorkerUnavailable,
    Cancelled,
    InvalidResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStatus {
    Ready,
    Updating,
    Failed,
}

/// This runs on a worker. The original entries and their byte-exact names
/// remain authoritative; lossy text is used only for deciding visibility.
pub fn project_filter(request: &FilterRequest) -> FilterResult {
    if request.text.is_empty() {
        return FilterResult { indices: (0..request.entries.len()).collect::<Vec<_>>().into(), entries: Arc::clone(&request.entries) };
    }
    let needle = comparable(&request.text);
    let indices = request.entries.iter().enumerate().filter_map(|(index, entry)| comparable(&entry.name().to_text_lossy()).contains(&needle).then_some(index)).collect::<Vec<_>>();
    let entries = indices.iter().map(|&index| request.entries[index].clone()).collect::<Vec<_>>();
    FilterResult { indices: indices.into(), entries: entries.into() }
}

fn comparable(text: &str) -> String {
    default_case_fold_str(text).nfd().filter(|character| !is_combining_mark(*character)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::{EntryKind, EntryName};

    fn request(text: &str, names: &[&[u8]]) -> FilterRequest {
        FilterRequest { browser: BrowserSide::Left, tab: TabId::new(1), listing: RequestToken::first(), visibility_generation: 0, generation: 1, text: text.into(), entries: names.iter().map(|name| Entry::new(EntryName::new(*name).unwrap(), EntryKind::File)).collect() }
    }

    #[test]
    fn canonical_accents_expansions_and_literal_spaces() {
        let names: &[&[u8]] = &["Café".as_bytes(), "cafe\u{301}".as_bytes(), "Straße".as_bytes(), b" a ", b"a"];
        assert_eq!(&*project_filter(&request("CAFE", names)).indices, &[0, 1]);
        assert_eq!(&*project_filter(&request("strasse", names)).indices, &[2]);
        assert_eq!(&*project_filter(&request(" a ", names)).indices, &[3]);
        assert_eq!(project_filter(&request("x", names)).entries.len(), 0);
        assert_eq!(project_filter(&request("", names)).entries.len(), names.len());
    }

    #[test]
    fn lossy_names_match_but_keep_distinct_byte_identities() {
        let request = request("�", &[b"a\xff", b"a\xfe"]);
        let result = project_filter(&request);
        assert_eq!(result.entries.len(), 2);
        assert_ne!(result.entries[0].name(), result.entries[1].name());
    }
}
