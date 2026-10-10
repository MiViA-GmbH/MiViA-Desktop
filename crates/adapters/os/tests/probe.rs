use mivia_os::process_table;

#[test]
fn the_table_lists_this_process_with_its_threads_and_memory() {
    let table = process_table().unwrap();
    let me = table
        .iter()
        .find(|row| row.pid == std::process::id())
        .unwrap();
    assert!(me.name.to_ascii_lowercase().starts_with("probe-"));
    assert!(me.threads.len() >= 2);
    assert!(me.private_bytes > 1 << 20);
    assert!(me.handles > 4);
    assert!(me.cycles > 0);
    assert!(me.threads.iter().map(|t| t.context_switches).sum::<u32>() > 0);
    assert!(table.iter().any(|row| row.pid == me.parent));
}
