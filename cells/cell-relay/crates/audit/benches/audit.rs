use agw_audit::contracts::audit_artifact::AuditArtifact;
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_parse_audit_artifact(c: &mut Criterion) {
    const DATA: &str = include_str!("../../../../../contracts/examples/audit_artifact.valid.json");
    c.bench_function("audit_artifact_parse", |b| {
        b.iter(|| {
            let _: Result<AuditArtifact, _> = serde_json::from_str(DATA);
        })
    });
}

criterion_group!(benches, bench_parse_audit_artifact);
criterion_main!(benches);
