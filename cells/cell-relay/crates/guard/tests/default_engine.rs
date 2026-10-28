use agw_guard::statistical::Engine;

#[test]
fn default_is_statistical_without_fabricated_windows() {
    assert!(matches!(Engine::default(), Engine::Statistical { .. }));
}
