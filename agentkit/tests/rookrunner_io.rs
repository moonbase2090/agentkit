use std::path::PathBuf;

use agentkit::rookrunner;

#[test]
fn rookrunner_fixture_lists_failed_and_reads_log() {
    let run_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/rookrunner-run");
    let failed = rookrunner::list_failed_jobs(&run_dir).unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].name, "test");
    let log = rookrunner::read_job_log(&run_dir, "test").unwrap();
    assert!(log.contains("FAILED"));
}
