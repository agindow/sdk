use agw_policy::{Class, Decision, Mode, Operation, Repair, decide_with_protection};

#[test]
fn protection_matrix_only_opens_unknown_repair_writes() {
    for closed in [true, false] {
        for mode in [Mode::Observe, Mode::Repair, Mode::Strict] {
            for op in [Operation::Read, Operation::Write, Operation::Unknown] {
                for class in [
                    None,
                    Some(Class::Shape),
                    Some(Class::Semantic),
                    Some(Class::Unknown),
                ] {
                    for repair in [Repair::Verified, Repair::Failed, Repair::Unavailable] {
                        let expected = if mode == Mode::Observe || class.is_none() {
                            Decision::Pass
                        } else if repair == Repair::Verified {
                            Decision::Repaired
                        } else if mode == Mode::Strict
                            || (op != Operation::Read
                                && (class == Some(Class::Semantic)
                                    || (closed && class == Some(Class::Unknown))))
                        {
                            Decision::Halt
                        } else {
                            Decision::Degraded
                        };
                        assert_eq!(
                            decide_with_protection(mode, op, class, repair, closed),
                            expected,
                            "{mode:?}/{op:?}/{class:?}/{repair:?}/{closed}"
                        );
                    }
                }
            }
        }
    }
}
