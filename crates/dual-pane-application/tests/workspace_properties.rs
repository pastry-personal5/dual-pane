//! Generated command and result sequences must preserve the workspace's
//! structural invariants, whatever order results arrive in.

use std::collections::HashMap;
use std::sync::Arc;

use dual_pane_application::{Command, Event, Input, Output, SettingsSnapshot, WorkRequest, Workspace, listing_changes};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingErrorKind, Location, RequestToken, SortDirection, SortField, SortSpec, TabId};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Navigate(bool, u8),
    Deliver { pick: usize, mask: u8, fail: bool },
    NewTab(bool),
    CloseTab(bool, usize),
    ActivateTab(bool, usize),
    ReorderTab(bool, usize, usize),
    Back(bool),
    Forward(bool),
    Refresh(bool),
    Sort(u8, bool),
    Select(bool, usize),
    Toggle(bool, usize),
    Range(bool, usize),
    Secondary(bool, usize),
    SelectAll(bool),
    Clear(bool),
    Parent(bool),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![(any::<bool>(), 0..4u8).prop_map(|(left, folder)| Op::Navigate(left, folder)), (any::<usize>(), any::<u8>(), any::<bool>()).prop_map(|(pick, mask, fail)| Op::Deliver { pick, mask, fail }), (any::<usize>(), any::<u8>(), any::<bool>()).prop_map(|(pick, mask, fail)| Op::Deliver { pick, mask, fail }), any::<bool>().prop_map(Op::NewTab), (any::<bool>(), any::<usize>()).prop_map(|(left, pick)| Op::CloseTab(left, pick)), (any::<bool>(), any::<usize>()).prop_map(|(left, pick)| Op::ActivateTab(left, pick)), (any::<bool>(), any::<usize>(), 0..6usize).prop_map(|(left, pick, position)| Op::ReorderTab(left, pick, position)), any::<bool>().prop_map(Op::Back), any::<bool>().prop_map(Op::Forward), any::<bool>().prop_map(Op::Refresh), (0..4u8, any::<bool>()).prop_map(|(folder, descending)| Op::Sort(folder, descending)), (any::<bool>(), any::<usize>()).prop_map(|(left, row)| Op::Select(left, row)), (any::<bool>(), any::<usize>()).prop_map(|(left, row)| Op::Toggle(left, row)), (any::<bool>(), any::<usize>()).prop_map(|(left, row)| Op::Range(left, row)), (any::<bool>(), any::<usize>()).prop_map(|(left, row)| Op::Secondary(left, row)), any::<bool>().prop_map(Op::SelectAll), any::<bool>().prop_map(Op::Clear), any::<bool>().prop_map(Op::Parent),]
}

fn side(left: bool) -> BrowserSide {
    if left { BrowserSide::Left } else { BrowserSide::Right }
}
fn folder(index: u8) -> Location {
    Location::root().join(&EntryName::new(format!("folder-{index}")).unwrap())
}
fn entries(mask: u8) -> Arc<[Entry]> {
    ["a", "b", "c", "d"].iter().enumerate().filter(|(bit, _)| mask & (1 << bit) != 0).map(|(_, name)| Entry::new(EntryName::new(*name).unwrap(), EntryKind::File)).collect()
}

/// One requested read with the Folder Items it asked the gateway to diff against.
type IssuedRead = (BrowserSide, TabId, RequestToken, Option<Arc<[Entry]>>);

/// Every read the workspace requested, and which one each tab still awaits.
#[derive(Default)]
struct Reads {
    issued: Vec<IssuedRead>,
    live: HashMap<(BrowserSide, TabId), RequestToken>,
}

impl Reads {
    fn record(&mut self, work: &[WorkRequest]) {
        for request in work {
            match request {
                WorkRequest::ReadDirectory { browser, tab, token, previous, .. } => {
                    self.issued.push((*browser, *tab, *token, previous.clone()));
                    self.live.insert((*browser, *tab), *token);
                }
                WorkRequest::Cancel { browser, tab, token } => {
                    if self.live.get(&(*browser, *tab)) == Some(token) {
                        self.live.remove(&(*browser, *tab));
                    }
                }
                WorkRequest::Operation(_) | WorkRequest::SaveSettings { .. } | WorkRequest::ProbeScreenshotsFolder { .. } | WorkRequest::ProbeFavoriteTarget { .. } | WorkRequest::LoadSettings | WorkRequest::ResetSettings => {}
            }
        }
    }
}

type Visible = (Option<Location>, Vec<Entry>, Vec<EntryName>);
fn visible(workspace: &Workspace, browser: BrowserSide) -> Visible {
    (workspace.location(browser).cloned(), workspace.entries(browser).to_vec(), workspace.selection(browser).entries().to_vec())
}

fn row_command(workspace: &Workspace, browser: BrowserSide, row: usize, make: fn(BrowserSide, TabId, usize, EntryName) -> Command) -> Option<Command> {
    let shown = workspace.entries(browser);
    let row = row.checked_rem(shown.len())?;
    Some(make(browser, workspace.active_tab(browser), row, shown[row].name().clone()))
}

fn command(workspace: &Workspace, op: &Op) -> Option<Command> {
    let pick = |browser: BrowserSide, index: usize| {
        let tabs = workspace.tabs(browser).collect::<Vec<_>>();
        tabs[index % tabs.len()]
    };
    Some(match *op {
        Op::Navigate(left, index) => Command::Navigate { browser: side(left), location: folder(index) },
        Op::NewTab(left) => Command::NewTab { browser: side(left) },
        Op::CloseTab(left, index) => Command::CloseTab { browser: side(left), tab: pick(side(left), index) },
        Op::ActivateTab(left, index) => Command::ActivateTab { browser: side(left), tab: pick(side(left), index) },
        Op::ReorderTab(left, index, position) => Command::ReorderTab { browser: side(left), tab: pick(side(left), index), position },
        Op::Back(left) => Command::GoBack { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Forward(left) => Command::GoForward { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Refresh(left) => Command::Refresh { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Sort(index, descending) => Command::SetSort { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), location: workspace.location(BrowserSide::Left).cloned().unwrap_or_else(|| folder(index)), sort: SortSpec::new(SortField::Name, if descending { SortDirection::Descending } else { SortDirection::Ascending }) },
        Op::Select(left, row) => return row_command(workspace, side(left), row, |browser, tab, row, name| Command::SelectEntry { browser, tab, row, name }),
        Op::Toggle(left, row) => return row_command(workspace, side(left), row, |browser, tab, row, name| Command::ToggleEntry { browser, tab, row, name }),
        Op::Range(left, row) => return row_command(workspace, side(left), row, |browser, tab, row, name| Command::SelectRange { browser, tab, row, name }),
        Op::Secondary(left, row) => return row_command(workspace, side(left), row, |browser, tab, row, name| Command::SecondarySelect { browser, tab, row, name }),
        Op::SelectAll(left) => Command::SelectAll { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Clear(left) => Command::ClearSelection { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Parent(left) => Command::GoToParent { browser: side(left), tab: workspace.active_tab(side(left)) },
        Op::Deliver { .. } => return None,
    })
}

fn check_structure(workspace: &Workspace) -> Result<(), TestCaseError> {
    for browser in [BrowserSide::Left, BrowserSide::Right] {
        let tabs = workspace.tabs(browser).collect::<Vec<_>>();
        prop_assert!(!tabs.is_empty());
        prop_assert!(tabs.contains(&workspace.active_tab(browser)));
        let mut unique = tabs.clone();
        unique.sort();
        unique.dedup();
        prop_assert_eq!(unique.len(), tabs.len());
        let shown = workspace.entries(browser).iter().map(|entry| entry.name().clone()).collect::<Vec<_>>();
        for selected in workspace.selection(browser).entries() {
            prop_assert!(shown.contains(selected), "selection names a Folder Item that is not shown");
        }
    }
    let left = workspace.tabs(BrowserSide::Left).collect::<Vec<_>>();
    prop_assert!(workspace.tabs(BrowserSide::Right).all(|tab| !left.contains(&tab)), "tab identities are shared between Browsers");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn generated_sequences_preserve_workspace_invariants(loaded in any::<bool>(), ops in proptest::collection::vec(op(), 1..80)) {
        let mut workspace = Workspace::new();
        let mut reads = Reads::default();
        if loaded {
            let transition = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
            reads.record(&transition.work);
        }
        for op in &ops {
            if let Op::Deliver { pick, mask, fail } = *op {
                if reads.issued.is_empty() {
                    continue;
                }
                let (browser, tab, token, previous) = reads.issued[pick % reads.issued.len()].clone();
                let live = reads.live.get(&(browser, tab)) == Some(&token);
                let before = (visible(&workspace, BrowserSide::Left), visible(&workspace, BrowserSide::Right));
                let shown_tab = workspace.active_tab(browser) == tab;
                // The gateway diffs on its worker against the rows the request carried.
                let event = if fail { Event::FolderItemsFailed { browser, tab, token, kind: ListingErrorKind::ItemMissing } } else { Event::FolderItemsLoaded { browser, tab, token, entries: entries(mask), changes: previous.map(|previous| listing_changes(&previous, &entries(mask))) } };
                let transition = workspace.handle(Input::from(event));
                reads.record(&transition.work);
                if live && shown_tab {
                    let old = if browser == BrowserSide::Left { &before.0.1 } else { &before.1.1 };
                    let new = workspace.entries(browser);
                    for output in &transition.outputs {
                        if let Output::FolderItemsLoaded { changes: Some(changes), .. } = output {
                            let mut rows = old.clone();
                            for change in changes {
                                prop_assert!(change.row + change.removed <= rows.len(), "a row change leaves the shown rows");
                                rows.splice(change.row..change.row + change.removed, new[change.row..change.row + change.inserted].iter().cloned());
                            }
                            prop_assert_eq!(&rows, &new.to_vec(), "applying the row changes to the shown rows does not yield the new rows");
                        }
                    }
                }
                if live {
                    reads.live.remove(&(browser, tab));
                } else {
                    prop_assert!(transition.outputs.is_empty(), "a stale result produced output");
                    prop_assert_eq!(&before, &(visible(&workspace, BrowserSide::Left), visible(&workspace, BrowserSide::Right)), "a stale result changed the workspace");
                }
            } else if let Some(command) = command(&workspace, op) {
                let transition = workspace.handle(command.into());
                reads.record(&transition.work);
            }
            check_structure(&workspace)?;
        }
    }
}
