use super::mutation_score;

#[test]
fn timeouts_count_as_detected_and_an_empty_run_scores_zero() {
    assert!((mutation_score(580, 108) - 84.302).abs() < 0.001);
    assert!((mutation_score(3, 1) - 75.0).abs() < f64::EPSILON);
    assert!(mutation_score(0, 0).abs() < f64::EPSILON);
}
