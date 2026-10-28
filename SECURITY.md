# Security policy

AGINDOW sits in the request path between agents and third-party APIs, so a
defect here can expose credentials or corrupt a write. Please report suspected
vulnerabilities privately.

## Reporting

Use GitHub's private vulnerability reporting on this repository
(**Security → Report a vulnerability**). If that channel is unavailable, open a
minimal public issue that asks for a private contact route and contains **no**
reproduction details, credentials or customer data.

Do not open a public issue with:

- API keys, tokens, cookies or webhook secrets
- customer payloads, prompts or production traffic
- a working exploit against a live deployment

Include, in the private report: affected package/version or commit, the path or
crate, impact, reproduction steps, and whether the issue is reachable in the
default configuration (statistical guest with no selected windows, observe mode) or only with explicit
opt-in features.

## Scope

In scope: `agindow` on npm and PyPI, the eleven relay crates, the WASM guest,
native/sidecar/Worker hosts, the Node/Python SDKs, and the public contracts in
this repository.

Out of scope: the hosted control plane, registry, dashboard and operations;
findings that require a compromised host or customer credentials; and claims
that detection is calibrated or production-qualified — it is not.

## Automated scanning

This repository runs GitHub code scanning (CodeQL for JavaScript/TypeScript and
Python), OpenSSF Scorecard, and dependency review on pull requests. Findings
appear under **Security → Code scanning**. They complement — they do not
replace — private vulnerability reports above. Dependency version bumps are
reviewed upstream and land here on the next export.

## What to expect

This is a pre-alpha project maintained by a small team. We will acknowledge a
complete private report as soon as practical, keep you informed while it is
triageed, and coordinate disclosure after a fix is available. We do not operate
a paid bounty programme and do not promise a fixed response SLA.

## Safe harbour

Good-faith research that avoids privacy violations, data destruction and service
disruption is welcome. Test against your own installation and synthetic
fixtures, not other customers' traffic or a live hosted endpoint.
