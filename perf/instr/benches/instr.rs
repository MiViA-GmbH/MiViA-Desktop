use std::hint::black_box;

use gungraun::prelude::*;
use mivia_domain::hold::{Access, Gate, HoldReason, LicenseScope, can_claim};
use mivia_domain::image_format::{FormatCheck, check_format};
use mivia_domain::natural::compare;
use mivia_domain::report::split_parts;

fn labels() -> Vec<String> {
    (0..1000)
        .rev()
        .map(|index| format!("Probe_{}-{index}.tif", index % 7))
        .collect()
}

fn jpeg_like(len: usize) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF];
    bytes.resize(len, 0x55);
    bytes.extend([0xFF, 0xD9]);
    bytes
}

#[library_benchmark]
#[bench::thousand(setup = labels)]
fn natural_sort(mut labels: Vec<String>) -> Vec<String> {
    labels.sort_by(|a, b| compare(a, b));
    black_box(labels)
}

#[library_benchmark]
#[bench::thousand(1000)]
fn part_split(members: usize) -> usize {
    black_box(split_parts(black_box(members)).len())
}

#[library_benchmark]
#[bench::jpeg(setup = jpeg_like, args = (1 << 20))]
fn format_check(bytes: Vec<u8>) -> bool {
    black_box(matches!(
        check_format(black_box(&bytes)),
        FormatCheck::Complete(_)
    ))
}

#[library_benchmark]
fn gate_predicate() -> bool {
    let holds = [
        HoldReason::ServiceUnavailable,
        HoldReason::LicenseInvalid(LicenseScope::Uploads),
    ];
    black_box(can_claim(black_box(Gate::StatusPoll), black_box(&holds)) == Access::Allowed)
}

library_benchmark_group!(
    name = domain,
    benchmarks = [natural_sort, part_split, format_check, gate_predicate]
);

main!(library_benchmark_groups = domain);
