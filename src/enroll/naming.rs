//! Deriving a cameras.yaml key for a newly enrolled camera.
//!
//! Axis serial numbers are the MAC address without separators
//! (`B8A44F55808F`), which is unique but tells you nothing when you later read
//! the config or a batch result. Pairing the model with the tail of the serial
//! keeps it recognisable and unique: `m2035-le-55808f`.

/// How many trailing serial characters to append.
const SERIAL_TAIL: usize = 6;

/// Turn a product name into a config key fragment.
///
/// `ProdNbr` is free text and really does contain spaces — "C Cube LW" is one
/// of Martin's cameras — so this lowercases, replaces any run of non
/// alphanumeric characters with a single dash, and trims dashes off the ends.
pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// Derive a camera name from model and serial number.
///
/// Falls back to the serial alone when the model is unusable, and to
/// `camera-<serial>` when even the serial is missing, so this never returns
/// something that cannot be a YAML key.
pub fn derive(model: &str, serial: &str) -> String {
    let model_slug = slugify(model);
    let serial_slug = slugify(serial);

    let tail = if serial_slug.len() > SERIAL_TAIL {
        &serial_slug[serial_slug.len() - SERIAL_TAIL..]
    } else {
        serial_slug.as_str()
    };

    match (model_slug.is_empty(), tail.is_empty()) {
        (false, false) => format!("{}-{}", model_slug, tail),
        (true, false) => format!("camera-{}", tail),
        (false, true) => model_slug,
        (true, true) => "camera".to_string(),
    }
}

/// Pick a name that is not already taken.
///
/// Collisions are near-impossible with a MAC tail, but two cameras from the
/// same batch can share it, and re-enrolling a replaced unit into a config that
/// still holds the old entry is entirely plausible. Falls back to the full
/// serial, then to numeric suffixes.
pub fn derive_unique(model: &str, serial: &str, taken: &[String]) -> String {
    let is_free = |n: &str| !taken.iter().any(|t| t == n);

    let base = derive(model, serial);
    if is_free(&base) {
        return base;
    }

    // Full serial rather than just the tail.
    let full = {
        let m = slugify(model);
        let s = slugify(serial);
        if m.is_empty() {
            format!("camera-{}", s)
        } else {
            format!("{}-{}", m, s)
        }
    };
    if is_free(&full) {
        return full;
    }

    for n in 2..1000 {
        let candidate = format!("{}-{}", base, n);
        if is_free(&candidate) {
            return candidate;
        }
    }
    unreachable!("could not find a free name after 1000 attempts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_cameras_from_martins_fleet() {
        // Model and serial read off the actual devices.
        assert_eq!(derive("M3045-V", "ACCC8E6D8D23"), "m3045-v-6d8d23");
        assert_eq!(derive("M3128-LVE", "B8A44FF1328B"), "m3128-lve-f1328b");
        assert_eq!(derive("M2035-LE", "B8A44F55808F"), "m2035-le-55808f");
        assert_eq!(derive("M1137 Mk II", "B8A44FF09697"), "m1137-mk-ii-f09697");
    }

    #[test]
    fn model_with_spaces() {
        // The case that would break a naive lowercase-only slug.
        assert_eq!(derive("C Cube LW", "ACCC8E68980B"), "c-cube-lw-68980b");
    }

    #[test]
    fn slugify_collapses_runs_and_trims() {
        assert_eq!(slugify("  M3045 -- V  "), "m3045-v");
        assert_eq!(slugify("AXIS  Q1615   Mk III"), "axis-q1615-mk-iii");
        assert_eq!(slugify("---"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn derived_names_are_valid_config_keys() {
        for (m, s) in [
            ("M3045-V", "ACCC8E6D8D23"),
            ("C Cube LW", "ACCC8E68980B"),
            ("M1137 Mk II", "B8A44FF09697"),
        ] {
            let n = derive(m, s);
            assert!(
                n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{} is not a valid key",
                n
            );
            assert!(!n.starts_with('-') && !n.ends_with('-'), "{} has a stray dash", n);
        }
    }

    #[test]
    fn missing_model_falls_back_to_serial() {
        assert_eq!(derive("", "B8A44F55808F"), "camera-55808f");
        assert_eq!(derive("???", "B8A44F55808F"), "camera-55808f");
    }

    #[test]
    fn missing_serial_falls_back_to_model() {
        assert_eq!(derive("M3045-V", ""), "m3045-v");
    }

    #[test]
    fn missing_everything_still_yields_a_key() {
        assert_eq!(derive("", ""), "camera");
    }

    #[test]
    fn short_serial_is_used_whole() {
        assert_eq!(derive("M3045-V", "ABC"), "m3045-v-abc");
    }

    #[test]
    fn collision_falls_back_to_full_serial_then_numbers() {
        let taken = vec!["m2035-le-55808f".to_string()];
        assert_eq!(
            derive_unique("M2035-LE", "B8A44F55808F", &taken),
            "m2035-le-b8a44f55808f"
        );

        let taken = vec![
            "m2035-le-55808f".to_string(),
            "m2035-le-b8a44f55808f".to_string(),
        ];
        assert_eq!(derive_unique("M2035-LE", "B8A44F55808F", &taken), "m2035-le-55808f-2");
    }

    #[test]
    fn no_collision_returns_the_plain_name() {
        let taken = vec!["west".to_string(), "entren".to_string()];
        assert_eq!(derive_unique("M2035-LE", "B8A44F55808F", &taken), "m2035-le-55808f");
    }
}
