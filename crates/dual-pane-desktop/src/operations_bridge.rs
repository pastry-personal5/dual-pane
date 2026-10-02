//! The Qt binder for file operations: Operation Panels and their Operation
//! Decision Cards, Permanent Delete confirmations, and the quit prompt.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use dual_pane_adapters::{OperationPanelViewModel, OperationsViewModel};
use dual_pane_application::Command;
use dual_pane_domain::{DecisionToken, OperationChoice, OperationId};

use crate::folder_items_list_model::operation_id;
use crate::workspace_bridge::{revision, with_session};

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(i32, panels_revision, cxx_name = "panelsRevision", READ, NOTIFY)]
        #[qproperty(i32, confirmations_revision, cxx_name = "confirmationsRevision", READ, NOTIFY)]
        #[qproperty(i32, quit_prompt_revision, cxx_name = "quitPromptRevision", READ, NOTIFY)]
        /// Milliseconds until `tick` has work, or -1 when nothing is due.
        #[qproperty(i32, next_deadline, cxx_name = "nextDeadline", READ, NOTIFY)]
        type OperationsModel = super::OperationsModelRust;

        /// Emitted after any call that may have changed what a binder shows.
        #[qsignal]
        #[cxx_name = "sessionChanged"]
        fn session_changed(self: Pin<&mut OperationsModel>);

        fn refresh(self: Pin<&mut OperationsModel>);
        /// Reveals and hides panels whose delay passed.
        fn tick(self: Pin<&mut OperationsModel>);

        /// Panels in the order their jobs started. Jobs are named by their
        /// non-negative ID.
        #[cxx_name = "panelCount"]
        fn panel_count(self: &OperationsModel) -> i32;
        #[cxx_name = "panelId"]
        fn panel_id(self: &OperationsModel, panel: i32) -> i64;
        #[cxx_name = "panelTitle"]
        fn panel_title(self: &OperationsModel, panel: i32) -> QString;
        #[cxx_name = "panelDetail"]
        fn panel_detail(self: &OperationsModel, panel: i32) -> QString;
        #[cxx_name = "panelStatus"]
        fn panel_status(self: &OperationsModel, panel: i32) -> QString;
        #[cxx_name = "panelBytes"]
        fn panel_bytes(self: &OperationsModel, panel: i32) -> QString;
        /// Entries handled, and planned or -1 while the plan is unknown.
        #[cxx_name = "panelDone"]
        fn panel_done(self: &OperationsModel, panel: i32) -> i32;
        #[cxx_name = "panelTotal"]
        fn panel_total(self: &OperationsModel, panel: i32) -> i32;
        #[cxx_name = "panelFinished"]
        fn panel_finished(self: &OperationsModel, panel: i32) -> bool;
        #[cxx_name = "panelCanCancel"]
        fn panel_can_cancel(self: &OperationsModel, panel: i32) -> bool;
        #[cxx_name = "panelHasDecision"]
        fn panel_has_decision(self: &OperationsModel, panel: i32) -> bool;
        #[cxx_name = "decisionMessage"]
        fn decision_message(self: &OperationsModel, panel: i32) -> QString;
        #[cxx_name = "decisionChoiceCount"]
        fn decision_choice_count(self: &OperationsModel, panel: i32) -> i32;
        #[cxx_name = "decisionChoiceLabel"]
        fn decision_choice_label(self: &OperationsModel, panel: i32, choice: i32) -> QString;
        /// 0 Replace, 1 Try Again, 2 Skip, 3 Cancel.
        #[cxx_name = "decisionChoiceCode"]
        fn decision_choice_code(self: &OperationsModel, panel: i32, choice: i32) -> i32;
        #[cxx_name = "decisionOffersApplyToAll"]
        fn decision_offers_apply_to_all(self: &OperationsModel, panel: i32) -> bool;
        /// Identifies the shown card, so a click on a replaced card is ignored.
        #[cxx_name = "decisionToken"]
        fn decision_token(self: &OperationsModel, panel: i32) -> i64;

        #[cxx_name = "cancelOperation"]
        fn cancel_operation(self: Pin<&mut OperationsModel>, job: i64);
        fn decide(self: Pin<&mut OperationsModel>, job: i64, token: i64, choice: i32, apply_to_all: bool);
        /// The person closed a panel; only a finished one goes away.
        #[cxx_name = "closePanel"]
        fn close_panel(self: Pin<&mut OperationsModel>, job: i64);

        #[cxx_name = "confirmationCount"]
        fn confirmation_count(self: &OperationsModel) -> i32;
        #[cxx_name = "confirmationId"]
        fn confirmation_id(self: &OperationsModel, confirmation: i32) -> i64;
        #[cxx_name = "confirmationTargets"]
        fn confirmation_targets(self: &OperationsModel, confirmation: i32) -> i32;
        #[cxx_name = "confirmDelete"]
        fn confirm_delete(self: Pin<&mut OperationsModel>, job: i64, targets: i32);

        /// Asks to quit; returns whether quitting may proceed now. Otherwise
        /// the quit prompt opens with `quitPromptRunning` operations.
        #[cxx_name = "requestQuit"]
        fn request_quit(self: Pin<&mut OperationsModel>) -> bool;
        /// The person confirmed quitting: every operation is cancelled.
        #[cxx_name = "confirmQuit"]
        fn confirm_quit(self: Pin<&mut OperationsModel>) -> bool;
        #[cxx_name = "quitPromptRunning"]
        fn quit_prompt_running(self: &OperationsModel) -> i32;
    }
}

#[derive(Default)]
pub struct OperationsModelRust {
    panels_revision: i32,
    confirmations_revision: i32,
    quit_prompt_revision: i32,
    next_deadline: i32,
    shown: OperationsViewModel,
}

/// The Qt code of each choice.
const CHOICES: [OperationChoice; 4] = [OperationChoice::Replace, OperationChoice::TryAgain, OperationChoice::Skip, OperationChoice::Cancel];

fn qt_int(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn job_code(id: OperationId) -> i64 {
    i64::try_from(id.get()).unwrap_or(-1)
}

impl ffi::OperationsModel {
    fn refresh(mut self: Pin<&mut Self>) {
        let Some((view, deadline)) = with_session(|session| (session.operations_view().clone(), session.next_operations_deadline())) else { return };
        let panels = revision(view.panels_revision());
        let confirmations = revision(view.confirmations_revision());
        let quit = revision(view.quit_prompt_revision());
        let deadline = deadline.map_or(-1, |deadline| i32::try_from(deadline.as_millis()).unwrap_or(i32::MAX));
        self.as_mut().rust_mut().get_mut().shown = view;
        if self.rust().panels_revision != panels {
            self.as_mut().rust_mut().get_mut().panels_revision = panels;
            self.as_mut().panels_revision_changed();
        }
        if self.rust().confirmations_revision != confirmations {
            self.as_mut().rust_mut().get_mut().confirmations_revision = confirmations;
            self.as_mut().confirmations_revision_changed();
        }
        if self.rust().quit_prompt_revision != quit {
            self.as_mut().rust_mut().get_mut().quit_prompt_revision = quit;
            self.as_mut().quit_prompt_revision_changed();
        }
        // A repeated deadline is reported again, so the view restarts its timer.
        self.as_mut().rust_mut().get_mut().next_deadline = deadline;
        self.as_mut().next_deadline_changed();
    }
    fn tick(mut self: Pin<&mut Self>) {
        with_session(|session| session.tick_operations());
        self.as_mut().session_changed();
    }

    fn panel(&self, panel: i32) -> Option<&OperationPanelViewModel> {
        usize::try_from(panel).ok().and_then(|panel| self.rust().shown.panels().get(panel))
    }
    fn panel_count(&self) -> i32 {
        qt_int(self.rust().shown.panels().len())
    }
    fn panel_id(&self, panel: i32) -> i64 {
        self.panel(panel).map_or(-1, |panel| job_code(panel.id))
    }
    fn panel_title(&self, panel: i32) -> QString {
        self.panel(panel).map_or_else(QString::default, |panel| QString::from(panel.title.as_str()))
    }
    fn panel_detail(&self, panel: i32) -> QString {
        self.panel(panel).map_or_else(QString::default, |panel| QString::from(panel.detail.as_str()))
    }
    fn panel_status(&self, panel: i32) -> QString {
        self.panel(panel).map_or_else(QString::default, |panel| QString::from(panel.status.as_str()))
    }
    fn panel_bytes(&self, panel: i32) -> QString {
        self.panel(panel).and_then(|panel| panel.bytes.as_deref()).map_or_else(QString::default, QString::from)
    }
    fn panel_done(&self, panel: i32) -> i32 {
        self.panel(panel).and_then(|panel| panel.progress).map_or(0, |(done, _)| qt_int(done))
    }
    fn panel_total(&self, panel: i32) -> i32 {
        self.panel(panel).and_then(|panel| panel.progress).map_or(-1, |(_, total)| qt_int(total))
    }
    fn panel_finished(&self, panel: i32) -> bool {
        self.panel(panel).is_some_and(|panel| panel.finished)
    }
    fn panel_can_cancel(&self, panel: i32) -> bool {
        self.panel(panel).is_some_and(|panel| panel.can_cancel)
    }
    fn panel_has_decision(&self, panel: i32) -> bool {
        self.panel(panel).is_some_and(|panel| panel.decision.is_some())
    }
    fn decision_message(&self, panel: i32) -> QString {
        self.panel(panel).and_then(|panel| panel.decision.as_ref()).map_or_else(QString::default, |card| QString::from(card.message.as_str()))
    }
    fn decision_choice_count(&self, panel: i32) -> i32 {
        self.panel(panel).and_then(|panel| panel.decision.as_ref()).map_or(0, |card| qt_int(card.choices.len()))
    }
    fn decision_choice(&self, panel: i32, choice: i32) -> Option<&dual_pane_adapters::ChoiceViewModel> {
        self.panel(panel)?.decision.as_ref()?.choices.get(usize::try_from(choice).ok()?)
    }
    fn decision_choice_label(&self, panel: i32, choice: i32) -> QString {
        self.decision_choice(panel, choice).map_or_else(QString::default, |choice| QString::from(choice.label))
    }
    fn decision_choice_code(&self, panel: i32, choice: i32) -> i32 {
        self.decision_choice(panel, choice).and_then(|choice| CHOICES.iter().position(|candidate| *candidate == choice.choice)).map_or(-1, qt_int)
    }
    fn decision_offers_apply_to_all(&self, panel: i32) -> bool {
        self.panel(panel).and_then(|panel| panel.decision.as_ref()).is_some_and(|card| card.offers_apply_to_all)
    }

    fn decision_token(&self, panel: i32) -> i64 {
        self.panel(panel).and_then(|panel| panel.decision.as_ref()).map_or(-1, |card| i64::try_from(card.token.get()).unwrap_or(-1))
    }

    fn cancel_operation(self: Pin<&mut Self>, job: i64) {
        if let Some(id) = operation_id(job) {
            self.submit(Command::CancelOperation { id });
        }
    }
    fn decide(mut self: Pin<&mut Self>, job: i64, token: i64, choice: i32, apply_to_all: bool) {
        let (Some(id), Ok(token), Some(choice)) = (operation_id(job), u64::try_from(token), usize::try_from(choice).ok().and_then(|choice| CHOICES.get(choice))) else { return };
        with_session(|session| session.decide(id, DecisionToken::new(token), *choice, apply_to_all));
        self.as_mut().session_changed();
    }
    fn close_panel(mut self: Pin<&mut Self>, job: i64) {
        if let Some(id) = operation_id(job) {
            with_session(|session| session.close_panel(id));
            self.as_mut().session_changed();
        }
    }

    fn confirmation_count(&self) -> i32 {
        qt_int(self.rust().shown.confirmations().len())
    }
    fn confirmation_id(&self, confirmation: i32) -> i64 {
        usize::try_from(confirmation).ok().and_then(|index| self.rust().shown.confirmations().get(index)).map_or(-1, |(id, _)| job_code(*id))
    }
    fn confirmation_targets(&self, confirmation: i32) -> i32 {
        usize::try_from(confirmation).ok().and_then(|index| self.rust().shown.confirmations().get(index)).map_or(0, |(_, targets)| qt_int(*targets))
    }
    fn confirm_delete(self: Pin<&mut Self>, job: i64, targets: i32) {
        if let (Some(id), Ok(targets)) = (operation_id(job), usize::try_from(targets)) {
            self.submit(Command::ConfirmPermanentDelete { id, targets });
        }
    }

    fn request_quit(mut self: Pin<&mut Self>) -> bool {
        let accepted = with_session(|session| session.request_quit(false)).unwrap_or(true);
        self.as_mut().session_changed();
        accepted
    }
    fn confirm_quit(mut self: Pin<&mut Self>) -> bool {
        let accepted = with_session(|session| session.request_quit(true)).unwrap_or(true);
        self.as_mut().session_changed();
        accepted
    }
    fn quit_prompt_running(&self) -> i32 {
        self.rust().shown.quit_prompt().map_or(0, qt_int)
    }

    fn submit(mut self: Pin<&mut Self>, command: Command) {
        with_session(|session| session.submit(command));
        self.as_mut().session_changed();
    }
}
