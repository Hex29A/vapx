//! What a running MCP server lets its clients do.
//!
//! The `confirm` argument of the dangerous tools is filled in by the calling
//! model, so it protects against mistakes but not against a prompt injection
//! (text in a log, an overlay or an event that talks the model into setting
//! it). The server-side policy here is what an injection cannot change:
//!
//! * `--mode` / `VAPX_MCP_MODE`: `read` offers only the read-only tools,
//!   `write` adds the tools that change settings, `all` adds the disruptive
//!   ones too (reboot, siren, relays, firmware, parameters, ...).
//!   The default is `read` over HTTP and `all` over stdio, where the client is
//!   started by the person who owns the cameras.
//! * `--allow` / `VAPX_MCP_ALLOW`: a comma-separated list of tool names. When
//!   set, only those tools are offered, and only if the mode also permits them.
//!
//! A tool that is not permitted is left out of `tools/list` and refused when
//! called by name.

use std::collections::BTreeSet;

use clap::ValueEnum;

/// How much a server may do.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum Mode {
    /// Only tools that change nothing
    Read,
    /// Also tools that change settings (but not the disruptive ones)
    Write,
    /// Every tool, including reboot, siren, relays, firmware and parameters
    All,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Read => "read",
            Mode::Write => "write",
            Mode::All => "all",
        }
    }
}

/// What a tool does, which decides the lowest mode that offers it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Changes nothing on the camera
    Read,
    /// Changes a setting or state, and can be put back
    Write,
    /// Disrupts, destroys or reaches outside the camera
    Disruptive,
}

impl Class {
    fn needs(self) -> Mode {
        match self {
            Class::Read => Mode::Read,
            Class::Write => Mode::Write,
            Class::Disruptive => Mode::All,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Policy {
    mode: Mode,
    allow: Option<BTreeSet<String>>,
}

impl Policy {
    /// No restrictions, for tests.
    #[cfg(test)]
    pub fn all() -> Self {
        Self { mode: Mode::All, allow: None }
    }

    /// `allow` is the parsed `VAPX_MCP_ALLOW`; an empty list counts as unset.
    pub fn new(mode: Mode, allow: Vec<String>) -> Self {
        let set: BTreeSet<String> = allow.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        Self { mode, allow: if set.is_empty() { None } else { Some(set) } }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn allowlist(&self) -> Option<&BTreeSet<String>> {
        self.allow.as_ref()
    }

    /// `Ok` when `tool` (of `class`) may be offered and called, otherwise the
    /// reason, worded for the client.
    pub fn check(&self, tool: &str, class: Class) -> Result<(), String> {
        if let Some(allow) = &self.allow {
            if !allow.contains(tool) {
                return Err(format!("{tool} is not available: this server only offers the tools on its allowlist (VAPX_MCP_ALLOW)"));
            }
        }
        if class.needs() > self.mode {
            return Err(format!(
                "{tool} is not available: this server runs in {} mode and {tool} needs {} mode (VAPX_MCP_MODE). Ask the person who runs the server.",
                self.mode.name(),
                class.needs().name()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(mode: Mode, allow: &[&str]) -> Policy {
        Policy::new(mode, allow.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn each_mode_offers_what_it_should() {
        let cases = [
            (Mode::Read, [true, false, false]),
            (Mode::Write, [true, true, false]),
            (Mode::All, [true, true, true]),
        ];
        for (mode, [read, write, disruptive]) in cases {
            let policy = p(mode, &[]);
            assert_eq!(policy.check("t", Class::Read).is_ok(), read, "{mode:?} read");
            assert_eq!(policy.check("t", Class::Write).is_ok(), write, "{mode:?} write");
            assert_eq!(policy.check("t", Class::Disruptive).is_ok(), disruptive, "{mode:?} disruptive");
        }
    }

    #[test]
    fn the_allowlist_narrows_and_the_mode_still_applies() {
        let policy = p(Mode::Write, &["ptz_move", "reboot_camera", " get_users "]);
        assert!(policy.check("ptz_move", Class::Write).is_ok());
        assert!(policy.check("get_users", Class::Read).is_ok(), "names are trimmed");
        assert!(policy.check("get_snapshot", Class::Read).unwrap_err().contains("allowlist"));
        // On the list, but the mode does not reach it.
        assert!(policy.check("reboot_camera", Class::Disruptive).unwrap_err().contains("needs all mode"));
    }

    #[test]
    fn an_empty_allowlist_means_no_allowlist() {
        let policy = p(Mode::All, &["", "  "]);
        assert!(policy.allowlist().is_none());
        assert!(policy.check("anything", Class::Read).is_ok());
    }
}
