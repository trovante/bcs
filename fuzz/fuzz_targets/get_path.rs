#![no_main]

use bcs_core::Decoder;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let split = 1 + (data[0] as usize % (data.len().saturating_sub(1).max(1)));
    let (path_bytes, file_bytes) = data.split_at(split.min(data.len()));
    let Ok(path) = std::str::from_utf8(path_bytes) else {
        return;
    };
    if path.is_empty() || path.len() > 512 {
        return;
    }
    if let Ok(mut decoder) = Decoder::from_bytes(file_bytes) {
        let _ = decoder.get(path);
    }
});
