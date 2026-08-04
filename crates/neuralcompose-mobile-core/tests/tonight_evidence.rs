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
        //
        // PENDING RE-EVALUATION (2026-08-04, ADR-003): bge-small-en-v1.5 is now
        // the named candidate, so the comment above describes the designation on
        // 2026-08-03, not the one in force today. `candidate_model_executed`
        // stays `false` deliberately: ADR-003 declines to promote a row as a side
        // effect of naming a candidate, because the rung would then be attained
        // by relabelling with nothing new run. Flipping this to `true` promotes
        // both linux rows to DeviceValidated — do it only as a deliberate act
        // with an evidence read behind it, not to make a stale comment agree.
        fixture_runtime_executed: true,
        candidate_model_executed: false,
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
fn the_function_now_agrees_with_the_prose_about_this_evidence() {
    // This test previously documented a DEFECT: the table reserves
    // `DeviceValidated` for "the real candidate model executed on named
    // physical hardware", but `attained_support_status` had no notion of which
    // model ran and granted that rung to a fixture run on named hardware —
    // exactly tonight's evidence. It was more permissive than the table it is
    // described as enforcing.
    //
    // `candidate_model_executed` closes it. Tonight's evidence names the
    // device, OS and backend and still stops at RuntimeSmokeValidated, because
    // the model that ran was bge-small-en-v1.5, a fixture.
    //
    // 2026-08-04 (ADR-003): bge-small-en-v1.5 is now the candidate, so this
    // guard no longer separates two different files — it separates a smoke run
    // from a device validation of the same weights. That is a weaker guard than
    // it was, and the distinction now has to be carried by the evidence read
    // rather than by the model filename. The assertion below is unchanged.
    assert_eq!(
        attained_support_status(tonight()),
        Some(SupportStatus::RuntimeSmokeValidated),
        "a fixture run must not reach DeviceValidated however well named"
    );
    assert!(!supports_claim(tonight(), SupportStatus::DeviceValidated));

    // The rung is still reachable — the fix restricts it, it does not remove
    // it. Swapping the fixture for the candidate on the same hardware promotes.
    let mut with_candidate = tonight();
    with_candidate.candidate_model_executed = true;
    assert_eq!(
        attained_support_status(with_candidate),
        Some(SupportStatus::DeviceValidated)
    );
}

#[test]
fn the_new_field_can_only_lower_an_attained_rung_never_raise_it() {
    // `candidate_model_executed` defaults to false for every existing caller,
    // so this change can only make a claim MORE conservative. That is the safe
    // direction for a promotion checker: an unmigrated caller loses a rung it
    // was not entitled to, rather than silently gaining one.
    let mut e = tonight();
    for candidate in [false, true] {
        e.candidate_model_executed = candidate;
        let attained = attained_support_status(e.clone()).expect("contracts pass");
        let without = {
            let mut w = e.clone();
            w.candidate_model_executed = false;
            attained_support_status(w).expect("contracts pass")
        };
        assert!(
            without <= attained,
            "clearing the flag must never raise the rung"
        );
    }
}
