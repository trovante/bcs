#![no_main]

use bcs_core::Decoder;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut decoder) = Decoder::from_bytes(data) {
        let _ = decoder.decode_to_value();
        let _ = decoder.schema();
        let _ = decoder.index_table();
        let _ = decoder.to_json();
    }
});
