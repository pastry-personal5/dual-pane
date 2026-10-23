//! The expanded navigator's observed read identity and result mailbox.

use super::*;
use crate::runtime::FolderChoiceRequest;

pub struct ColumnReadRequest {
    pub browser: BrowserSide,
    pub tab: TabId,
    pub source: Location,
    pub visible_revision: u64,
    pub column: usize,
    pub after: Option<EntryName>,
    pub generation: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ColumnContext {
    generation: u64,
    tab: TabId,
    source: Location,
    visible_revision: u64,
    visibility: VisibilityPolicy,
}

fn slot(browser: BrowserSide) -> usize {
    match browser {
        BrowserSide::Left => 0,
        BrowserSide::Right => 1,
    }
}

impl<R: WorkRunner> WorkspaceSession<R> {
    pub fn request_folder_choices(&mut self, read: ColumnReadRequest) -> Option<u64> {
        let ColumnReadRequest { browser, tab, source, visible_revision, column, after, generation } = read;
        let view = self.view(browser);
        if view.active_tab() != Some(tab) || view.location() != Some(&source) || view.visible_revision() != visible_revision {
            return None;
        }
        let targets = source.breadcrumb_targets();
        let parent = targets.get(column)?.clone();
        let selected = source.components().get(column)?.clone();
        let context = ColumnContext { generation, tab, source: source.clone(), visible_revision, visibility: self.workspace_view().visibility() };
        if self.column_contexts[slot(browser)].as_ref() != Some(&context) {
            self.cancel_column_context(browser);
            self.column_contexts[slot(browser)] = Some(context.clone());
        }
        self.next_column_request = self.next_column_request.wrapping_add(1);
        let id = self.next_column_request;
        let request = FolderChoiceRequest { id, browser, tab, source, visible_revision, parent, selected, after, visibility: context.visibility, generation };
        if let Some(result) = self.runner.request_folder_choices(request) {
            self.accept_column_result(result);
        }
        Some(id)
    }

    pub fn cancel_folder_choices(&mut self, browser: BrowserSide, generation: u64) {
        if self.column_contexts[slot(browser)].as_ref().is_some_and(|context| context.generation == generation) {
            self.cancel_column_context(browser);
        }
    }

    pub fn take_folder_choice_result(&mut self, id: u64) -> Option<FolderChoiceResult> {
        self.column_results.remove(&id)
    }

    fn cancel_column_context(&mut self, browser: BrowserSide) {
        if let Some(context) = self.column_contexts[slot(browser)].take() {
            self.runner.cancel_folder_choices(browser, context.generation);
            self.column_results.retain(|_, result| !(result.request.browser == browser && result.request.generation == context.generation));
        }
    }

    pub(super) fn refresh_column_contexts(&mut self) {
        for browser in [BrowserSide::Left, BrowserSide::Right] {
            let Some(context) = self.column_contexts[slot(browser)].as_ref() else { continue };
            let view = self.view(browser);
            if view.active_tab() != Some(context.tab) || view.location() != Some(&context.source) || view.visible_revision() != context.visible_revision || self.workspace_view().visibility() != context.visibility {
                self.cancel_column_context(browser);
            }
        }
    }

    pub(super) fn accept_column_result(&mut self, result: FolderChoiceResult) {
        let browser = result.request.browser;
        let valid = self.column_contexts[slot(browser)].as_ref().is_some_and(|context| context.generation == result.request.generation && context.tab == result.request.tab && context.source == result.request.source && context.visible_revision == result.request.visible_revision && context.visibility == result.request.visibility);
        if valid {
            self.column_results.insert(result.request.id, result);
        }
    }
}
