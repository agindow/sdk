# Local source-fix export
```sh
agindow export-source-fix ./export-request.json \
  --as-of 2026-10-20T12:00:01Z > ./source-fix.json
```
The request contains `checkout`, `binding` and `grant` from the existing
, plus `evidenceReference`:
an explicit local JSON filename resolved relative to the request file.
No remote URL, reference resolution, source execution, in-place edit or publisher
is configured. Files are bounded and final symlinks refused. Local grants remain
maintainer assertions, not hosted authentication or current provider authority.

`exportSourceFix(request, now, source)` accepts an injected
`LocalEvidenceSource.read(reference)` for a host-controlled local evidence store.
It detaches inputs, bounds/waits for evidence, and runs the existing deterministic
recipe with its independent AST preservation checks. The output includes the
minimal patch, candidate digest receipt, reviewed fact/provenance citations and
explicit `tests.state: unvalidated`, with no fabricated test receipt. There is no
default independent validation runner; the separate host must supply one.
Provider CI, vendor acceptance and business effects remain unverified, and
`publicationAuthorized` is false. Refusals keep their typed reasons and omit
private source/errors. Exit codes are 0 for generated/no-op and 1 for refusal.
Artifacts may contain private source context; persistence and disclosure remain
caller-owned. No hosted publisher is needed for this local export.
