# Testnet-to-Mainnet Whitepaper
## ENBLOCK · Crystal Signal Capsule · Formal Logic Blueprint

**Document class:** Architecture whitepaper and release-gate specification  
**Status:** Proposed; no testnet or mainnet deployment is asserted  
**Repository:** SteelsSystem/MetaEye-Void  
**Baseline:** Enblock Memory Diagram, segment digest and provenance proposal  
**Protocol family:** KoDeX I X O · LQ-1 · Needle & SteelsSystem

---

## 0. Executive statement

This document preserves the Enblock formal-logic blueprint and defines a staged path from a deterministic local prototype to a public testnet and, only after independent verification, a possible mainnet release.

**Crystal Signal Capsule (CSC)** names a proposed signed, content-addressed envelope for transmitting a bounded signal or state claim. “Crystal” is a protocol metaphor for a stable, inspectable representation. It does not imply quantum communication, physical crystal storage, quantum entanglement, or any unimplemented capability.

**No mainnet launch date is promised.** The timeline is a sequence of exit-gated phases with indicative durations, not a launch commitment. A failed gate stops progression.

## 1. System model and trust boundaries

The architecture is separated into six planes:

1. **Source plane:** Git repository, immutable commit, tree and blob identifiers.
2. **Intake plane:** allowlisted read-only access to explicitly selected files.
3. **Integrity plane:** whole-file digest, fixed-size segment digests and Enblock Merkle root.
4. **Logic plane:** deterministic parser, bounded state transitions and policy evaluation.
5. **Signal plane:** CSC encoding, signing, verification and replay control.
6. **Ledger plane:** append-only receipts, signed checkpoints and independently verifiable provenance.

A Git identity, content digest, policy approval and author identity are distinct facts. None implies the others.

### Architecture flow

Git ref → Pin immutable commit → Read commit tree → Select exact blob → Bounded read-only intake → Segment hashes → Merkle root and file digest → Formal logic and policy evaluation → Crystal Signal Capsule → Signature and replay checks → Local ledger receipt → Independent verification → Testnet gate → Mainnet readiness review → Separately authorized release.

## 2. Formal Logic Blueprint — preserved core

### 2.1 Four-valued input semantics

The system must not collapse absence, emptiness, numeric zero and unknown semantics into one value.

| Symbol | State | Meaning | Default action |
|---|---|---|---|
| MISSING | No input | No value was supplied | HOLD or reject by contract |
| EMPTY | Empty input | A value exists with zero bytes | Process only if allowed |
| ZERO | Explicit zero | Numeric value zero was supplied | Process as zero |
| UNKNOWN | Unbound semantics | Representation exists, meaning is undefined | HOLD |

UNKNOWN never implies permission.

### 2.2 Observer separation

Let Xₜ be the observed object state, Oₜ the observer state, Eₜ the observation event and Lₜ the ledger state.

Observation is modelled as:

M(Xₜ, Oₜ) = (Xₜ, Oₜ₊₁, Eₜ)

Recording an observation must not silently mutate the observed target. If observation causes a target-side effect, that effect must be represented as a separate explicit transition.

The ledger records the observation event and its provenance. It must not rewrite the digest of the originally observed bytes.

### 2.3 Finite Vortex exploration

For state Xₜ, policy provides a finite action set A(Xₜ). A transition is:

Xₜ₊₁ = T(Xₜ, a), where a ∈ A(Xₜ).

Suggested prototype limits:

| Parameter | Bound |
|---|---:|
| Beam width | 64 |
| Maximum depth | 8 |
| Maximum branching factor | 3 |
| Maximum expansions | 1,536 |
| Segment size | 65,536 bytes |
| Network egress in local mode | 0 |

Worst-case expansion count:

N_expand ≤ 64 × 8 × 3 = 1,536.

These are configuration targets. They become compliance claims only after implementation tests establish that limits are enforced.

### 2.4 Determinism

For fixed input bytes, pinned configuration, policy version and explicit seed:

F(I, C, P, S) = R.

Repeated runs must produce the same canonical result R. Wall-clock timestamps, host-specific paths, random session identifiers and nondeterministic scheduling data must not be included in the deterministic result digest. These may be recorded separately as audit metadata.

### 2.5 BinaryGate

| Value | State | Allowed behaviour |
|---:|---|---|
| 0 | OFF_HALT | Reject execution; return configured halt status |
| 1 | ON_GO | Local, allowlisted diagnostics and bounded simulation |
| 2 | TEST | Non-production tests and dry-run policy evaluation |

Invariants:

- G = 0 implies execution is forbidden.
- G = 1 implies operations are a subset of the local allowlist.
- G = 2 implies no production commit.

An atomic variable alone does not make a multi-step authorization workflow atomic. Each sensitive operation must check a stable gate/configuration generation at its enforcement boundary.

## 3. Enblock segment integrity

### 3.1 Hash-layer separation

| Layer | Purpose |
|---|---|
| Git commit OID | Identifies a repository commit object |
| Git tree OID | Identifies a repository tree |
| Git blob OID | Identifies a Git blob object |
| Whole-file SHA-256 | Identifies raw file bytes |
| Segment SHA-256 | Identifies one exact byte range |
| Enblock Merkle root | Commits to the ordered set of segments |
| Provenance digest | Binds the content claim to repository metadata |

Git object identifiers are not equivalent to raw-file SHA-256. Git blob hashing includes Git object framing. A Git OID must therefore be treated as a distinct identifier.

### 3.2 Segment leaf

Default segment size is 65,536 bytes. Segment bytes are hashed exactly as received. Do not normalize line endings, Unicode, whitespace or source code.

For segment index i, byte length lenᵢ and raw bytes bᵢ:

    leaf_i = SHA256(
      0x00 || "ENBLOCK-SEG-v1\0" ||
      u64be(i) || u32be(len_i) || b_i
    )

### 3.3 Merkle parent and root

    parent = SHA256(
      0x01 || "ENBLOCK-NODE-v1\0" ||
      left_hash || right_hash
    )

At an odd-width level, promote the unpaired final node unchanged. Do not silently duplicate it.

    enblock_root = SHA256(
      0x02 || "ENBLOCK-ROOT-v1\0" ||
      u64be(total_byte_length) ||
      u32be(segment_count) ||
      tree_root
    )

For an empty file:

    tree_root = SHA256("ENBLOCK-EMPTY-v1\0")
    segment_count = 0
    total_byte_length = 0

The same root envelope is used. Format version, integer widths, byte order, odd-node rule and empty-input rule are mandatory parts of the protocol.

A separate cross-check is retained:

    file_sha256 = SHA256(raw_file_bytes)

### 3.4 Bounded-memory requirement

The target local implementation reads one segment at a time, hashes it, retains its digest and releases or reuses the segment buffer. A streaming Merkle frontier can reduce working digest storage to O(log n). A flat list of all leaf hashes uses O(n) digest storage and must be described honestly.

The GitHub connector may materialize a complete response before local code receives it. The streaming memory guarantee therefore applies only to an implementation that actually streams bytes at the transport/intake boundary.

## 4. Crystal Signal Capsule (CSC)

### 4.1 Purpose

A CSC is a versioned, canonical, signed envelope carrying a bounded signal claim and its integrity/provenance references. It is not a transport protocol by itself and does not establish trust merely by existing.

Required properties:
- deterministic serialization;
- domain-separated content and provenance binding;
- authenticated origin;
- freshness and replay controls;
- bounded size and field validation;
- explicit policy scope and expiry;
- verifiable rejection reasons.

### 4.2 Required capsule fields

| Field | Type | Requirement |
|---|---|---|
| schema | text | Fixed value csc.v1 |
| network_domain | text | local, testnet or explicitly named environment |
| issuer_key_id | bytes/text | Key identifier, never secret key material |
| sequence | unsigned integer | Monotonic within issuer scope |
| nonce | bytes | Unique cryptographically random value |
| issued_at | integer | Unix seconds, audit/freshness field |
| expires_at | integer | Explicit expiry |
| message_type | text | Registered type |
| payload_digest | 32 bytes | Digest of separately bounded payload |
| enblock_root | 32 bytes | Optional Enblock root reference |
| provenance_digest | 32 bytes | Optional provenance binding |
| policy_version | text | Policy used to authorize the signal |
| max_uses | unsigned integer | Replay/use bound |
| signature_algorithm | text | Explicit algorithm identifier |
| signature | bytes | Signature over canonical unsigned capsule |

The capsule does not carry passwords, private keys, access tokens or unnecessary raw repository content.

### 4.3 Canonical signing preimage

Use deterministic CBOR (RFC 8949 deterministic encoding profile) or another fully specified canonical binary format. Do not sign ad hoc JSON text.

Let Cᵤ be the capsule with the signature field omitted:

    signing_digest = SHA256(
      "CRYSTAL-SIGNAL-CAPSULE-v1\0" ||
      canonical_cbor(C_u)
    )

Sign according to the selected signature scheme's exact specification. Do not invent an Ed25519 prehash mode: use the documented Ed25519 message-signing semantics, or explicitly adopt and test a standard prehash construction.

The verifier:
1. checks schema and canonical encoding;
2. validates network domain and issuer key status;
3. checks issued/expiry bounds against trusted time policy;
4. checks nonce and sequence against replay state;
5. validates payload and Enblock digests;
6. verifies the signature;
7. evaluates authorization separately from cryptographic validity.

A valid signature proves that the holder of the corresponding private key signed the covered message. It does not prove that the payload is safe, factually correct or authorized for every action.

### 4.4 Capsule state machine

RECEIVED → PARSED → FRESHNESS_CHECK → REPLAY_CHECK → SIGNATURE_CHECK → POLICY_CHECK → ACCEPTED or HOLD → LEDGERED.

Malformed or oversized input is REJECTED. Expired data is EXPIRED. Reused nonce/sequence is REPLAY_REJECTED. Invalid signatures are SIGNATURE_REJECTED.

All terminal decisions carry stable reason codes. Secret material must never be included in diagnostic errors.

## 5. Testnet compliance plan

### Gate T0 — Specification freeze

Evidence required:
- protocol schemas versioned;
- domain separators and canonical serialization specified;
- input and output limits explicit;
- threat model and trust boundaries reviewed;
- no ambiguous handling of UNKNOWN;
- golden test vectors for empty, one-segment, multi-segment and odd-leaf Merkle cases.

Exit criterion: an independent implementer can reproduce every specified digest and capsule preimage.

### Gate T1 — Local deterministic implementation

Evidence required:
- parser and segmenter operate on raw bytes;
- repeated fixed-input runs produce identical roots;
- oversized input and integer overflow fail closed;
- malformed or non-canonical CBOR is rejected;
- no repository content is executed;
- test suite covers all BinaryGate states.

Exit criterion: unit and property tests pass on x86-64 and ARM64 with identical protocol outputs.

### Gate T2 — Adversarial and fault testing

Evidence required:
- mutation of one byte changes the expected digest/root;
- reordering segments changes the root;
- truncated and appended blobs are detected;
- odd-node and empty-file rules have golden vectors;
- replayed nonce/sequence rejected;
- invalid, revoked and wrong-domain signing keys rejected;
- crash/restart preserves ledger consistency;
- resource exhaustion limits enforced;
- dependency and secret scanning reviewed.

Exit criterion: no unresolved critical/high findings; accepted findings have documented owners and mitigations.

### Gate T3 — Public or isolated testnet

Evidence required:
- testnet uses separate keys, identifiers, state and endpoints from production;
- no production credentials or value-bearing assets;
- monitoring, structured audit and incident response exercised;
- rate limits and denial-of-service controls tested;
- node/client upgrades and rollback tested;
- independent observers can validate capsule and ledger receipts.

Exit criterion: testnet completes a declared observation period with agreed service/security objectives, no unexplained consensus or state divergence, and a published test report.

### Gate T4 — Mainnet readiness review

Evidence required:
- independent security review and remediation report;
- cryptographic protocol review;
- deterministic cross-platform test report;
- recovery, backup and key-rotation drills;
- operational runbooks and on-call ownership;
- upgrade and rollback plan;
- governance and change-control policy;
- legal/compliance review appropriate to the actual deployment;
- signed release artifacts and reproducible build evidence;
- explicit human authorization from accountable release owners.

Exit criterion: all critical gates are signed off. A testnet pass alone never authorizes mainnet.

## 6. Indicative timeline — gate based, not a promise

The following planning envelope assumes a small engineering team and an existing repository. Calendar dates must be assigned only after owners, scope and capacity are confirmed.

| Phase | Indicative duration | Deliverable | Gate |
|---|---:|---|---|
| P0 — Specification freeze | 1–2 weeks | Protocol v1, threat model, golden vectors | T0 |
| P1 — Local implementation | 2–4 weeks | Segment engine, Merkle reducer, CSC codec/verifier | T1 |
| P2 — Adversarial validation | 2–3 weeks | Fuzz/property tests, cross-platform results, security fixes | T2 |
| P3 — Testnet deployment | 2–4 weeks | Isolated testnet, telemetry, drills and public report | T3 |
| P4 — Mainnet readiness review | 2–4 weeks | Independent audits, governance and operational sign-off | T4 |
| P5 — Controlled launch window | Scheduled only after T4 | Limited rollout, monitoring and rollback readiness | Separate authorization |

Indicative time before a launch decision: **9–17 weeks**, excluding material redesign, audit remediation, external approvals and operational delays. This is not a claim that the codebase is ready or that a mainnet launch will occur in that interval.

Release rule:

    MAINNET_ELIGIBLE = T0 AND T1 AND T2 AND T3 AND T4
    NOT MAINNET_ELIGIBLE => HOLD

A release decision must be bound to an immutable commit, build provenance, test-report digests, policy version and signed approval record.

## 7. Security and operational invariants

1. Unknown semantics never imply allow.
2. No testnet key is accepted as a mainnet authority.
3. No secret key is committed to Git, artifact logs or ledger payloads.
4. Every capsule has an explicit domain, issuer, sequence, nonce and expiry.
5. Replay checks are persistent and scoped to issuer and network domain.
6. Authorization is distinct from signature validity.
7. Every release artifact is tied to an immutable source commit and reproducible build record.
8. Failed or unavailable verification results in HOLD, not optimistic acceptance.
9. Local mode performs zero network egress unless a separately authorized operation changes the mode.
10. Mainnet activation requires explicit, separately audited release authorization.

## 8. Threat model and residual risks

| Threat | Control | Residual risk |
|---|---|---|
| Mutable Git ref changes during intake | Pin commit before reading tree/blob | Compromised upstream account or malicious reviewed commit |
| Segment order or bytes changed | Domain-separated indexed leaf hashes and Merkle root | Hash algorithm compromise or implementation defect |
| Capsule replay | Nonce, sequence, expiry and persistent replay store | State rollback or inconsistent replay stores |
| Key theft | Protected signer, rotation and revocation | Compromised signer environment |
| Malformed input/resource exhaustion | Size, depth, count and time budgets | Parser bugs and platform-specific behaviour |
| False provenance claim | Pinned Git objects and signed checkpoints | Signing authority itself may be compromised |
| Unsafe source code | No execution during intake; separate review | Review cannot prove absence of all vulnerabilities |
| Operator error | Separate environments, approvals and runbooks | Collusion or governance failure |

## 9. Required repository deliverables

Suggested implementation sequence:

    src/
      enblock/
        mod.rs
        segment.rs
        merkle.rs
        provenance.rs
        receipt.rs
      signal/
        mod.rs
        capsule.rs
        canonical.rs
        signer.rs
        verifier.rs
        replay.rs
      release/
        gate.rs
        manifest.rs

    tests/
      vectors/
        enblock-v1.json
        csc-v1.json
      segment_golden.rs
      merkle_properties.rs
      capsule_roundtrip.rs
      capsule_replay.rs
      cross_platform_determinism.rs

    docs/
      ENBLOCK_MEMORY_DIAGRAM.md
      TESTNET_MAINNET_WHITEPAPER.md

The existing manifest-generation script must not be treated as proof of release integrity without verifying that it includes every intended source/configuration file, excludes generated and secret material, handles paths safely and checks against a trusted expected manifest. A script that regenerates a manifest from the current working tree cannot independently prove that the working tree is trustworthy.

## 10. Acceptance checklist

- [ ] Protocol v1 schemas frozen and versioned.
- [ ] Golden vectors published for segment and Merkle hashing.
- [ ] Canonical capsule vectors published.
- [ ] Signature and replay tests pass.
- [ ] Cross-platform deterministic output verified.
- [ ] No-execution intake enforced.
- [ ] Resource and fault limits tested.
- [ ] Independent security review completed.
- [ ] Testnet incident and rollback exercises completed.
- [ ] Reproducible build and signed provenance available.
- [ ] Governance and operational ownership documented.
- [ ] Explicit mainnet authorization recorded.

## 11. Signal capsule example — illustrative only

    {
      "schema": "csc.v1",
      "network_domain": "testnet",
      "issuer_key_id": "<public-key-id>",
      "sequence": 1,
      "nonce": "<random-bytes-base64url>",
      "issued_at": 0,
      "expires_at": 0,
      "message_type": "enblock.digest.attestation",
      "payload_digest": "<32-byte-digest>",
      "enblock_root": "<32-byte-merkle-root>",
      "provenance_digest": "<32-byte-provenance-digest>",
      "policy_version": "LQ-1-testnet-1",
      "max_uses": 1,
      "signature_algorithm": "Ed25519",
      "signature": "<signature-bytes-base64url>"
    }

Placeholders are not valid production values. This is a schema illustration, not a signed signal.

## 12. Status and publication note

This whitepaper is an architectural proposal. It does not claim:
- an existing deployed testnet;
- a deployed mainnet;
- an implemented Crystal Signal Capsule;
- completion of security, cryptographic or legal audits;
- guaranteed cryptographic erasure or absolute memory non-residency;
- a confirmed launch date.

The next engineering milestone is to implement Enblock segment/Merkle primitives and CSC canonicalization against published golden vectors, then pass T1 before any testnet deployment work is represented as complete.
