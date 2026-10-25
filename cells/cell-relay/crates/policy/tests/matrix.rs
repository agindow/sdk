use agw_policy::{Class, Decision, Mode, Operation, Repair, decide};

#[test]
fn complete_matrix_keeps_observe_availability_and_semantic_precedence_separate() {
    for mode in [Mode::Observe, Mode::Repair, Mode::Strict] {
        for op in [Operation::Read, Operation::Write, Operation::Unknown] {
            for class in [
                None,
                Some(Class::Shape),
                Some(Class::Semantic),
                Some(Class::Unknown),
            ] {
                for repair in [Repair::Unavailable, Repair::Failed, Repair::Verified] {
                    let actual = decide(mode, op, class, repair);
                    let expected = if mode == Mode::Observe || class.is_none() {
                        Decision::Pass
                    } else if repair == Repair::Verified {
                        Decision::Repaired
                    } else if mode == Mode::Strict
                        || (op != Operation::Read && class != Some(Class::Shape))
                    {
                        Decision::Halt
                    } else {
                        Decision::Degraded
                    };
                    assert_eq!(actual, expected, "{mode:?}/{op:?}/{class:?}/{repair:?}");
                }
            }
        }
    }
}
