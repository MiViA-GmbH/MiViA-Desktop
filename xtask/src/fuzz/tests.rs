use super::shard_of;

#[test]
fn every_target_lands_in_exactly_one_shard() {
    let targets: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let shards: Vec<Vec<String>> = (1..=3).map(|shard| shard_of(&targets, shard, 3)).collect();
    assert_eq!(shards[0], ["a", "d", "g"]);
    assert_eq!(shards[1], ["b", "e"]);
    assert_eq!(shards[2], ["c", "f"]);
    assert_eq!(shard_of(&targets, 1, 0), Vec::<String>::new());
}
