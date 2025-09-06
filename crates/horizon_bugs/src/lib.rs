use bug::{init_handle, template_file, BugReportHandle};

pub fn get_bugs() -> BugReportHandle {
    let bug_report_handle = init_handle("myorg", "shared-project")
        .add_template_file("crash", template_file!("../templates/crash_report.md", labels: ["bug", "crash"]))
        .add_template_file("performance", template_file!("../templates/performance_issue.md", labels: ["performance", "optimization"]));

    bug_report_handle
}