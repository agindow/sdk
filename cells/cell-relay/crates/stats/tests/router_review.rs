use agw_stats::router::{Error, Label, Reviews};

#[test]
fn only_reviewed_cases_count_and_all_four_labels_have_cells() -> Result<(), Error> {
    let mut reviews = Reviews::default();
    assert_eq!(reviews.accuracy(), None);
    assert_eq!(reviews.reviewed(), 0);
    let labels = [
        Label::Shape,
        Label::Semantic,
        Label::Invariant,
        Label::Unknown,
    ];
    for truth in labels {
        for predicted in labels {
            reviews.record(predicted, truth)?;
            assert_eq!(reviews.count(truth, predicted), 1);
        }
    }
    assert_eq!(reviews.reviewed(), 16);
    assert_eq!(reviews.accuracy(), Some(0.25));
    reviews.record(Label::Shape, Label::Semantic)?;
    assert_eq!(reviews.count(Label::Semantic, Label::Shape), 2);
    assert_eq!(reviews.count(Label::Shape, Label::Semantic), 1);
    assert_eq!(reviews.accuracy(), Some(4. / 17.));
    Ok(())
}

#[test]
fn review_counter_overflow_is_atomic() -> Result<(), Error> {
    let mut counts = [[0; 4]; 4];
    counts[0][0] = u64::MAX;
    let mut reviews = Reviews::from_counts(counts)?;
    let before = reviews.clone();
    assert_eq!(
        reviews.record(Label::Shape, Label::Shape),
        Err(Error::CounterOverflow)
    );
    assert_eq!(reviews, before);
    assert_eq!(
        reviews.record(Label::Unknown, Label::Semantic),
        Err(Error::CounterOverflow)
    );
    assert_eq!(reviews, before);
    counts[1][0] = 1;
    assert_eq!(Reviews::from_counts(counts), Err(Error::CounterOverflow));
    Ok(())
}

#[test]
fn restored_review_counts_preserve_independent_truth_rows() -> Result<(), Error> {
    let mut counts = [[0; 4]; 4];
    counts[1][0] = 3; // truth semantic, predicted shape
    counts[0][0] = 1;
    let mut reviews = Reviews::from_counts(counts)?;
    assert_eq!(reviews.count(Label::Semantic, Label::Shape), 3);
    assert_eq!(reviews.count(Label::Shape, Label::Semantic), 0);
    assert_eq!(reviews.reviewed(), 4);
    assert_eq!(reviews.accuracy(), Some(0.25));
    reviews.record(Label::Unknown, Label::Invariant)?;
    assert_eq!(reviews.count(Label::Invariant, Label::Unknown), 1);
    assert_eq!(reviews.count(Label::Unknown, Label::Invariant), 0);
    Ok(())
}
