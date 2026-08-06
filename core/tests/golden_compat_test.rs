use bcs_core::{Decoder, Encoder};

const GOLDEN_TEST_JSON: &str = r#"{
  "server": {
    "host": "localhost",
    "port": 8080,
    "ssl": {
      "enabled": true,
      "cert_path": "/path/to/cert.pem"
    }
  },
  "database": {
    "name": "myapp",
    "connections": 10,
    "timeout": 30
  },
  "features": ["auth", "logging", "metrics"],
  "debug": false
}"#;

// Snapshot generated from deterministic compact profile (from repo root):
// mkdir -p tmp && printf '%s' '<GOLDEN_TEST_JSON>' > tmp/golden_payload.json
// cargo run -p bcs-cli -- encode tmp/golden_payload.json -o tmp/golden_compact1.bcs --compact
// python3 -c "import base64;print(base64.b64encode(open('tmp/golden_compact1.bcs','rb').read()).decode())"
const GOLDEN_COMPACT_BCS_BASE64: &str = "RlNDQgEAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAKwEAAAAAAADWd2PdLupxMkIEAAAAMAhkYXRhYmFzZVh0LKMxlQOLQgMAAAAwC2Nvbm5lY3Rpb25zr8EfP+iw3kgSCgAAADAEbmFtZW39eUoue19sMAVteWFwcDAHdGltZW91dAbY3jNoo2H6Eh4AAAAwBWRlYnVnL8z5pqJR/QEBMAhmZWF0dXJlc2VSMphHKolpQAMAAAAwBGF1dGgwB2xvZ2dpbmcwB21ldHJpY3MwBnNlcnZlciBz81GUghPiQgMAAAAwBGhvc3SYthL0aSv7RzAJbG9jYWxob3N0MARwb3J0O/Akj8XhqR8SkB8AADADc3Nsq2OBj8KJBXNCAgAAADAJY2VydF9wYXRoH/IRyxJY07wwES9wYXRoL3RvL2NlcnQucGVtMAdlbmFibGVkAPFbydF0JBwC";

// Snapshot generated from deterministic default profile (from repo root):
// cargo run -p bcs-cli -- encode tmp/golden_payload.json -o tmp/golden_default_det1.bcs
// python3 -c "import base64;print(base64.b64encode(open('tmp/golden_default_det1.bcs','rb').read()).decode())"
const GOLDEN_DEFAULT_BCS_BASE64: &str = "RlNDQgEAAQBAAAAAAAAAAMgAAAAAAAAACAEAAAAAAACXAAAAAAAAAJ8BAAAAAAAA4wAAAAAAAADJFYxL2iGZ2hUBAAD0D5ajMS4wgaRSb290gaZTdHJ1Y3SEqGRhdGFiYXNllhMA8BCDq2Nvbm5lY3Rpb25zlqVJbnQzMsPAkMDApG5hbWWWOQAxaW5nEgCIp3RpbWVvdXQmAAEZAMGlZGVidWeWpEJvb2wRAPgBqGZlYXR1cmVzloGkTGlzdEYAdqZzZXJ2ZXKBAFmkaG9zdGkASaRwb3JmAEWjc3NsMQC5gqljZXJ0X3BhdGg2AIenZW5hYmxlZIgAAY0AAQUAgKRSb290gICABAAAABAAAAAAAEA/AAAAACBz81GUghPiaAAAAAAAAAD/////BgAAAHNlcnZlcgUAAABlUjKYRyqJaUsAAAAAAAAA/////wgAAABmZWF0dXJlcwgAAABYdCyjMZUDiwAAAAAAAAAA/////wgAAABkYXRhYmFzZQ8AAAAvzPmmolH9AUoAAAAAAAAA/////wUAAABkZWJ1Z0IDAAAAMAtjb25uZWN0aW9uc6/BHz/osN5IEgoAAAAwBG5hbWVt/XlKLntfbDAFbXlhcHAwB3RpbWVvdXQG2N4zaKNh+hIeAAAAAUADAAAAMARhdXRoMAdsb2dnaW5nMAdtZXRyaWNzQgMAAAAwBGhvc3SYthL0aSv7RzAJbG9jYWxob3N0MARwb3J0O/Akj8XhqR8SkB8AADADc3Nsq2OBj8KJBXNCAgAAADAJY2VydF9wYXRoH/IRyxJY07wwES9wYXRoL3RvL2NlcnQucGVtMAdlbmFibGVkAPFbydF0JBwC";

#[test]
fn golden_compact_encoding_matches_snapshot() {
    let mut encoder = Encoder::new();
    encoder.set_compact_mode(true);
    let encoded = encoder
        .encode_from_json(GOLDEN_TEST_JSON)
        .expect("Failed to encode golden JSON payload");

    let golden = base64_decode(GOLDEN_COMPACT_BCS_BASE64).expect("Invalid golden base64");

    assert_eq!(
        encoded, golden,
        "Golden snapshot mismatch. If intentional, update docs/compatibility-policy.md and regenerate snapshot."
    );
}

#[test]
fn golden_compact_snapshot_decodes_successfully() {
    let golden = base64_decode(GOLDEN_COMPACT_BCS_BASE64).expect("Invalid golden base64");
    let mut decoder = Decoder::from_bytes(&golden).expect("Failed to create decoder from golden");

    let decoded = decoder
        .to_json()
        .expect("Failed to decode golden snapshot to JSON");

    assert!(decoded.contains("\"server\""));
    assert!(decoded.contains("\"database\""));
    assert!(decoded.contains("\"features\""));
}

#[test]
fn golden_default_encoding_matches_snapshot() {
    let mut encoder = Encoder::new();
    let encoded = encoder
        .encode_from_json(GOLDEN_TEST_JSON)
        .expect("Failed to encode golden JSON payload");

    let golden = base64_decode(GOLDEN_DEFAULT_BCS_BASE64).expect("Invalid golden base64");

    assert_eq!(
        encoded, golden,
        "Golden default snapshot mismatch. If intentional, update docs/compatibility-policy.md and regenerate snapshot."
    );
}

#[test]
fn golden_default_snapshot_decodes_successfully() {
    let golden = base64_decode(GOLDEN_DEFAULT_BCS_BASE64).expect("Invalid golden base64");
    let mut decoder = Decoder::from_bytes(&golden).expect("Failed to create decoder from golden");

    let decoded = decoder
        .to_json()
        .expect("Failed to decode golden snapshot to JSON");

    assert!(decoded.contains("\"server\""));
    assert!(decoded.contains("\"database\""));
    assert!(decoded.contains("\"features\""));
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const INVALID: u8 = 0xFF;
    let mut table = [INVALID; 256];
    for (i, c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[*c as usize] = i as u8;
    }

    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("Invalid base64 length".to_string());
    }

    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    let mut i = 0;
    while i < bytes.len() {
        let c0 = bytes[i];
        let c1 = bytes[i + 1];
        let c2 = bytes[i + 2];
        let c3 = bytes[i + 3];

        let v0 = table[c0 as usize];
        let v1 = table[c1 as usize];
        if v0 == INVALID || v1 == INVALID {
            return Err("Invalid base64 character".to_string());
        }

        let pad2 = c2 == b'=';
        let pad3 = c3 == b'=';

        let v2 = if pad2 {
            0
        } else {
            let v = table[c2 as usize];
            if v == INVALID {
                return Err("Invalid base64 character".to_string());
            }
            v
        };

        let v3 = if pad3 {
            0
        } else {
            let v = table[c3 as usize];
            if v == INVALID {
                return Err("Invalid base64 character".to_string());
            }
            v
        };

        let b0 = (v0 << 2) | (v1 >> 4);
        out.push(b0);

        if !pad2 {
            let b1 = ((v1 & 0x0F) << 4) | (v2 >> 2);
            out.push(b1);
        }

        if !pad3 {
            let b2 = ((v2 & 0x03) << 6) | v3;
            out.push(b2);
        }

        i += 4;
    }

    Ok(out)
}
