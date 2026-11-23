//! The release binaries trust the roots embedded in `webpki-roots`, not the operating system's.
//! 1.0.10 dropped emSign ECC Root CA - G3 (distrusted in October 2026); an older locked version
//! would put it back, for example after a careless `cargo update` of the lock file.
//!
//! The lock file may hold more than one entry: `webpki-roots` 0.26.x is a thin shim (used by
//! tungstenite) that only re-exports 1.x, so it carries no roots of its own.

fn entries(lock: &str, name: &str) -> Vec<(Vec<u64>, String)> {
    let header = format!("name = \"{name}\"\n");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lock[from..].find(&header) {
        let start = from + at;
        let block_end = lock[start..].find("\n\n").map(|n| start + n).unwrap_or(lock.len());
        let block = &lock[start..block_end];
        let line = block.lines().nth(1).expect("version line");
        let version = line.trim().strip_prefix("version = \"").and_then(|v| v.strip_suffix('"')).expect("version = \"x.y.z\"");
        out.push((version.split('.').map(|n| n.parse().expect("numeric version")).collect(), block.to_string()));
        from = block_end;
    }
    assert!(!out.is_empty(), "{name} not in Cargo.lock");
    out
}

#[test]
fn webpki_roots_is_new_enough_to_exclude_the_distrusted_emsign_root() {
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")).expect("read Cargo.lock");
    for (v, block) in entries(&lock, "webpki-roots") {
        if v[0] == 0 {
            // A shim: it must forward to a 1.x that is checked below.
            assert!(block.contains("\"webpki-roots 1."), "webpki-roots {v:?} is not a shim over 1.x:\n{block}");
        } else {
            assert!(v >= vec![1, 0, 10], "webpki-roots {v:?} still trusts emSign ECC Root CA - G3; need 1.0.10 or later");
        }
    }
}
