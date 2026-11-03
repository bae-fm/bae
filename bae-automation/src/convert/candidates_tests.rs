use super::*;

#[test]
fn import_step_and_phase_serialize_snake_case() {
    let preparing =
        AutomationImportStep::from_core(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles));
    let json = serde_json::to_value(preparing).unwrap();
    assert_eq!(json["kind"], "preparing");
    assert_eq!(json["step"], "validating_source_files");

    let running = AutomationImportStep::from_core(ImportStep::Running(ImportPhase::ReadingFiles));
    let json = serde_json::to_value(running).unwrap();
    assert_eq!(json["kind"], "running");
    assert_eq!(json["phase"], "reading_files");
}

#[test]
fn indeterminate_import_progress_serializes_without_a_fraction() {
    let status = automation_import_status(
        None,
        Some(&ImportInFlight { step: ImportStep::Preparing(PrepareStep::ValidatingSourceFiles) }),
        None,
    )
    .expect("the importing row has a status");

    let json = serde_json::to_value(status).unwrap();
    assert_eq!(json["progress_percent"], serde_json::Value::Null);
    assert_eq!(json["step"]["step"], "validating_source_files");
}
