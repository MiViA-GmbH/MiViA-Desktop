use super::{mutation_score, raised, replace_value};

#[test]
fn timeouts_count_as_detected_and_an_empty_run_scores_zero() {
    assert!((mutation_score(580, 108) - 84.302).abs() < 0.001);
    assert!((mutation_score(3, 1) - 75.0).abs() < f64::EPSILON);
    assert!(mutation_score(0, 0).abs() < f64::EPSILON);
}

#[test]
fn a_raised_floor_keeps_two_points_of_room_and_never_drops() {
    assert_eq!(raised(79, (964, 36)), 94);
    assert_eq!(raised(79, (80, 20)), 79);
    assert_eq!(raised(85, (60, 40)), 85);
}

#[test]
fn replacing_a_value_touches_only_its_section() {
    let text = "[coverage]
a = 1
[mutation]
a = 2
b = 3
";
    assert_eq!(
        replace_value(text, "mutation", "a", 9),
        "[coverage]
a = 1
[mutation]
a = 9
b = 3
"
    );
}
