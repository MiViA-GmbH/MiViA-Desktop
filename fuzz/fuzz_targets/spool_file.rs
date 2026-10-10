#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(failure) = mivia_application::fuzz::spool_file(data) {
        panic!("{failure}");
    }
});
