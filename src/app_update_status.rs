//! Read-only update fields in the UI's existing tmpfs status line. No effects or filesystem reads here.

use std::time::Instant;
use pocket_ui::{ui_status::Line, update::{self, observation::public_version}};
use super::{App, Updates, UpdatePhase, UpdateTask};

impl Updates {
    fn status_fields(&self, line: &mut Line) {
        let (phase, id, kind, version, worker, requested, cancellable) = match &self.phase {
            UpdatePhase::Idle => ("idle", None, "none", None, None, false, false),
            UpdatePhase::Confirming => ("confirming", None, "app", self.manifest.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.version.as_str()), None, false, false),
            UpdatePhase::Running(job) => {
                let (phase, kind, version) = match &job.task {
                    UpdateTask::Check => ("checking", "check", None),
                    UpdateTask::App { version, .. } => (job.worker.phase.code(), "app", Some(version.as_str())),
                    UpdateTask::System { version, .. } => (job.worker.phase.code(), "system", Some(version.as_str())),
                };
                (phase, Some(job.id), kind, version, Some(job.worker), job.task.cancel().is_some_and(|c| c.is_cancelled()), job.task.cancel().is_some())
            }
            UpdatePhase::Finished(report) => (report.outcome.code(), Some(report.id), report.target.kind(), report.target.version(), Some(report.worker), report.cancel_requested, false),
        };
        line.field("update", phase);
        line.field("update.job", id.map_or_else(|| "none".into(), |id| id.to_string()));
        line.field("update.kind", kind);
        line.field("update.version", version.and_then(public_version).unwrap_or("none"));
        let file = worker.filter(|w| w.scope == update::BytesScope::CurrentFile);
        line.field("update.bytes", file.map_or(0, |w| w.bytes).to_string());
        line.field("update.total", file.map_or(0, |w| w.total).to_string());
        line.field("update.bytes.scope", if file.is_some() { "current-file" } else { "none" });
        line.field("update.cancel", if !cancellable { "unavailable" } else if worker.is_some_and(|w| w.cancel_effective) { "effective" } else { "too-late" });
        line.field("update.cancel.requested", if requested { "yes" } else { "no" });
    }
}

impl App {
    pub(crate) fn update_status(&self, line: &mut Line, now: Instant) {
        self.updates.status_fields(line);
        if let Some((phase, bytes, total)) = self.menu.as_ref().and_then(|m| m.cheats.as_ref()).map(|cheats| cheats.download_status())
            .or_else(|| self.setup.as_ref().and_then(|s| s.cheat_status())) {
            line.field("cheats", phase);
            line.field("cheats.bytes", bytes.to_string());
            line.field("cheats.total", total.to_string());
        }
        let (notice, reason, version) = self.update_notice_status(now);
        line.field("update.notice", notice);
        line.field("update.notice.schedule", self.update_hint.schedule_code());
        line.field("update.notice.reason", reason);
        line.field("update.notice.version", version.and_then(public_version).unwrap_or("none"));
        let inspection = &self.update_inspection;
        line.field("update.milestone", inspection.milestone.code());
        line.field("update.health", inspection.health.code());
        line.field("update.pending", inspection.pending.code());
        line.field("update.pending.version", inspection.pending.version().and_then(public_version).unwrap_or("none"));
        line.field("update.recovery", inspection.recovery.code());
        line.field("update.recovery.layout", inspection.layout.code());
        line.field("update.stability.scope", inspection.stability_scope());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UpdateOutcome;

    fn fields(updates: &Updates) -> std::collections::BTreeMap<String, String> {
        let mut line = Line::new();
        updates.status_fields(&mut line);
        pocket_ui::ui_status::parse(&line.render(1, 1)).into_iter().collect()
    }

    #[test]
    fn a_late_cancel_and_a_finished_install_remain_distinct_in_the_projection() {
        let mut updates = Updates::default();
        let cancel = update::Cancel::new();
        updates.start(UpdateTask::App { version: "9.8.7".into(), total: 100, cancel: cancel.clone() });
        let initial = fields(&updates);
        assert_eq!(initial["update.job"], "1");
        assert_eq!(initial["update.cancel"], "effective");
        if let UpdatePhase::Running(job) = &mut updates.phase {
            job.worker = update::WorkerSnapshot { phase: update::WorkerPhase::Flushing, bytes: 100, total: 100, scope: update::BytesScope::CurrentFile, cancel_effective: false };
        }
        updates.request_cancel();
        let late = fields(&updates);
        assert_eq!(late["update"], "flushing");
        assert_eq!(late["update.cancel"], "too-late");
        assert_eq!(late["update.cancel.requested"], "yes");
        assert_eq!(late["update.bytes.scope"], "current-file");
        updates.finish(UpdateOutcome::Installed);
        let done = fields(&updates);
        assert_eq!(done["update"], "installed");
        assert_eq!(done["update.cancel"], "unavailable");
        assert_eq!(done["update.cancel.requested"], "yes");
        assert_eq!(done["update.job"], initial["update.job"]);
        assert_eq!(done["update.bytes"], "100");
        updates.start(UpdateTask::Check);
        let check = fields(&updates);
        assert_eq!(check["update.job"], "2");
        assert_eq!(check["update"], "checking");
        assert_eq!(check["update.cancel"], "unavailable");
        assert_eq!(check["update.bytes.scope"], "none");
        assert_eq!(check["update.bytes"], "0");
    }

    #[test]
    fn public_job_fields_do_not_copy_arbitrary_manifest_text() {
        let mut updates = Updates::default();
        updates.start(UpdateTask::App { version: "https://example.invalid/?token=secret".into(), total: 1, cancel: update::Cancel::new() });
        assert_eq!(fields(&updates)["update.version"], "none");
        updates.finish(UpdateOutcome::Failed);
        assert_eq!(fields(&updates)["update.version"], "none");
    }
}
