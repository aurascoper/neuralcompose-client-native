//! What rung does 2026-08-03's actual evidence attain?
//!
//! `docs/support-matrix.md` says `attained_support_status()` is "the
//! machine-checkable form of this table" and that "a row may never claim more
//! than that function returns for its evidence." So a promotion must be
//! checked against the function, not argued from the prose.

use neuralcompose_mobile_core::runtime_target::{
    attained_support_status, supports_claim, SupportEvidence, SupportStatus,
};

/// Exactly what the 2026-08-03 session established for the two linux rows.
fn tonight() -> SupportEvidence {
    SupportEvidence {
        contracts_and_tests_pass: true,
        builds_on_named_target: true,
        // A fixture model — bge-small-en-v1.5, not the real candidate.
        fixture_runtime_executed: true,
        physical_device: Some("GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M".into()),
        os_version: Some("Ubuntu 26.04 LTS, kernel 7.0.0-28-generic".into()),
        backend_version: Some("llama.cpp d0bfb1981266c271cd0536a8aa7c5e863e7cdf61".into()),
        // No signing, packaging, install, upgrade or removal exists.
        signed_packaging_accepted: false,
        acceptance_document: Some("docs/acceptance/llama-cpp-cpu-linux.md".into()),
    }
}

#[test]
fn the_promotion_being_applied_is_within_what_the_function_permits() {
    // The rule is a CEILING: a row may not claim more than this. Claiming
    // less is always permitted and is what the matrix is doing, because the
    // prose reserves DeviceValidated for "the real candidate model" and this
    // session ran a fixture.
    assert!(supports_claim(
        tonight(),
        SupportStatus::RuntimeSmokeValidated
    ));
}

#[test]
fn the_function_and_the_prose_disagree_about_this_evidence() {
    // DOCUMENTING A DEFECT, not asserting the desired behaviour.
    //
    // The table's prose says `DeviceValidated` means "the real candidate model
    // executed on named physical hardware". `attained_support_status` has no
    // notion of which model ran: it takes `fixture_runtime_executed` plus
    // named device/OS/backend and returns DeviceValidated. So a FIXTURE run on
    // named hardware — exactly tonight's evidence — reaches the rung the prose
    // reserves for the candidate model.
    //
    // This does not affect the promotion being applied, which claims the lower
    // rung deliberately. It matters because the function is described as the
    // machine-checkable form of the table and here it is more permissive than
    // the table, so a future row could be promoted to DeviceValidated on
    // fixture evidence and pass the check.
    assert_eq!(
        attained_support_status(tonight()),
        Some(SupportStatus::DeviceValidated),
        "if this ever returns RuntimeSmokeValidated the gap has been closed \
         and this test should be replaced by one asserting the fix"
    );
    // And the gap is exactly the missing distinction: with the hardware
    // unnamed, the same fixture evidence lands where the prose expects.
    let mut anonymous = tonight();
    anonymous.physical_device = None;
    assert_eq!(
        attained_support_status(anonymous),
        Some(SupportStatus::RuntimeSmokeValidated)
    );
}
