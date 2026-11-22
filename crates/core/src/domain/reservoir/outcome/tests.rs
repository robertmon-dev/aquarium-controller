use crate::domain::reservoir::*;

#[test]
fn soft_failures_latch_automation_and_faults_do_not() {
    for result in [
        FillResult::Timeout,
        FillResult::TdsUnavailable,
        FillResult::Blocked,
    ] {
        assert!(result.latches_auto());
        assert_eq!(result.fault(), None);
    }
    for result in [
        FillResult::Overtime,
        FillResult::Leak,
        FillResult::Completed,
        FillResult::Aborted,
    ] {
        assert!(!result.latches_auto());
    }
}

#[test]
fn an_interrupted_marker_becomes_a_reboot_record() {
    let record = FillRecord::interrupted(RunningMarker {
        trigger: FillTrigger::Auto,
        started_at: Some(7),
    });
    assert_eq!(record.result, FillResult::InterruptedByReboot);
    assert_eq!(record.started_at, Some(7));
    assert_eq!(record.trigger, FillTrigger::Auto);
}

#[test]
fn result_names_match_their_serialised_form() {
    let all = [
        FillResult::Completed,
        FillResult::OffTarget,
        FillResult::Overtime,
        FillResult::Timeout,
        FillResult::Aborted,
        FillResult::TdsUnavailable,
        FillResult::Leak,
        FillResult::Blocked,
        FillResult::InterruptedByReboot,
    ];
    for result in all {
        assert_eq!(
            serde_json::to_string(&result).expect("json"),
            format!("\"{}\"", result.name())
        );
    }
}
