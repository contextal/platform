#![no_main]

use libfuzzer_sys::fuzz_target;
use pgrules::parse_to_sql;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let _ = parse_to_sql(input);
});
