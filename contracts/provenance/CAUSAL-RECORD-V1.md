# Causal record v1

Additive proposed contract for the cellular pilot. ADR-004's existing
`ProvenanceEnvelope` and `AssertionKind` are embedded unchanged. This wrapper
introduces temporal semantics absent from v1; it does not add another evidence
taxonomy or make any value eligible for neural-memory ingestion.

The producer declares the source event time, first evidenced availability time,
decision cutoff, expiry, consuming policy version, lineage, and payload digest.
Every timestamp is UTC. Consumers enforce
`sourceTimestamp <= availableAt <= decisionCutoff < expiresAt`, plus
`decisionCutoff <= now < expiresAt`. Modification time, download time, and a
backfilled historical bar are not proof of historical availability. Missing
availability is a refusal, not a substituted source time.

`payloadSha256` hashes UTF-8 JSON with sorted keys, compact separators, finite
numbers only and ASCII escaping. Producers should use decimal strings for
quantities; this profile does not claim arbitrary floating-point canonicalization.
Lineage includes upstream strategies; a consumer must reject its own strategy
in lineage as independent corroboration. An empty list is an explicit producer
claim, not an inference the consumer may fill in.

The OBI consumer pins both schemas by file digest, resolves references offline,
and tests temporal and payload checks with rejecting controls. These checks
validate the producer's declaration; they cannot establish that it told the
truth. No existing envelope fixture, Rust wire type, or server contract changes.
