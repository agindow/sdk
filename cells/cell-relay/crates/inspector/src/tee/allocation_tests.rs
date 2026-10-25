//! Capacity transitions count Vec growth requests without a global allocator
//! (implementing GlobalAlloc would violate the workspace's unsafe-code ban).
//! Check after every read, including EOF and the uncaptured tail.

use super::*;
use std::io::Cursor;

const FRAME: usize = 64 * 1024;
const MIB: usize = 1024 * 1024;

#[test]
fn large_capture_has_bounded_growth() -> io::Result<()> {
    let mut tee = Tee::new(io::repeat(0xA5).take((4 * DEFAULT_CAP) as u64));
    let mut buffer = [0; FRAME];
    let mut previous = 0;
    let mut growths = 0;
    let mut requested = 0;
    while tee.read(&mut buffer)? != 0 {
        let capacity = tee.captured.capacity();
        if capacity != previous {
            growths += 1;
            requested += capacity;
            previous = capacity;
        }
    }
    assert_eq!(growths, 2, "growth requests");
    assert_eq!(requested, DEFAULT_CAP + FRAME, "total bytes requested");
    assert_eq!(tee.captured.len(), DEFAULT_CAP);
    assert!(tee.truncated());
    Ok(())
}

#[test]
fn tiny_and_oversized_reads_bound_total_and_transient_storage() -> io::Result<()> {
    for chunk in [1, FRAME - 1, FRAME + 1, DEFAULT_CAP + 1] {
        let mut tee = Tee::new(io::repeat(1).take((DEFAULT_CAP + 1) as u64));
        let mut buffer = vec![0; chunk];
        let mut previous = 0;
        let mut requested = 0;
        let mut growths = 0;
        while tee.read(&mut buffer)? != 0 {
            let capacity = tee.captured.capacity();
            if capacity != previous {
                // A realloc may keep both allocations alive until the copy
                // finishes. This bounds requested storage even in that case.
                assert!(previous + capacity <= DEFAULT_CAP + FRAME);
                requested += capacity;
                growths += 1;
                previous = capacity;
            }
        }
        assert!(growths <= 18, "chunk {chunk}: {growths} growths");
        assert!(
            requested < 2 * DEFAULT_CAP,
            "chunk {chunk}: {requested} bytes"
        );
    }
    Ok(())
}

#[test]
fn capacity_is_bounded_at_empty_threshold_and_large_bodies() -> io::Result<()> {
    for (size, bound) in [
        (0, 0),
        (FRAME - 1, FRAME),
        (FRAME, FRAME),
        (FRAME + 1, DEFAULT_CAP),
        (4 * DEFAULT_CAP, DEFAULT_CAP),
    ] {
        let mut tee = Tee::new(io::repeat(1).take(size as u64));
        let mut buffer = [0; FRAME];
        loop {
            let n = tee.read(&mut buffer)?;
            assert!(tee.captured.capacity() <= bound, "body {size}");
            if n == 0 {
                break;
            }
        }
    }
    Ok(())
}

#[test]
fn concurrent_threshold_crossings_have_a_bounded_combined_capacity() -> io::Result<()> {
    const IN_FLIGHT: usize = 16;
    let mut tees: Vec<_> = (0..IN_FLIGHT)
        .map(|_| Tee::new(io::repeat(1).take((FRAME + 1) as u64)))
        .collect();
    let mut buffer = [0; FRAME];
    for tee in &mut tees {
        while tee.read(&mut buffer)? != 0 {}
    }
    let total: usize = tees.iter().map(|tee| tee.captured.capacity()).sum();
    assert_eq!(total, IN_FLIGHT * DEFAULT_CAP);
    Ok(())
}

#[test]
fn typical_fixture_never_reserves_a_large_head() -> io::Result<()> {
    const REQUEST: &[u8] = include_bytes!("../../fixtures/typical_request.json");
    for chunk in [1, REQUEST.len(), FRAME] {
        let mut tee = Tee::new(Cursor::new(REQUEST));
        let mut buffer = vec![0; chunk];
        while tee.read(&mut buffer)? != 0 {
            assert!(tee.captured.capacity() <= REQUEST.len().next_power_of_two());
        }
        assert_eq!(tee.captured(), REQUEST);
    }
    Ok(())
}

#[test]
fn custom_caps_bound_capacity_even_with_tiny_reads() -> io::Result<()> {
    for cap in [0, 1, 63, FRAME - 1, FRAME + 1, 3 * MIB + 1] {
        let mut tee = Tee::with_cap(io::repeat(1).take((cap + 1) as u64), cap);
        let mut buffer = [0; 4093];
        while tee.read(&mut buffer)? != 0 {
            assert!(tee.captured.capacity() <= cap, "cap {cap}");
        }
        assert_eq!(tee.captured.len(), cap);
        assert!(tee.truncated());
    }
    Ok(())
}
