//! Confirmation for commands that erase or lock out something on a camera.

use std::io::{IsTerminal, Write};

/// Ask before `what` happens. `what` is a sentence without a final stop, like
/// "Removing user bob from 192.0.2.10 cannot be undone". `--yes` goes
/// through; without it a terminal gets a prompt that needs the word `yes`, and
/// anything else (a script, a pipe) is refused, because nobody can answer.
pub fn confirm(what: &str, yes: bool) -> anyhow::Result<()> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("{what}. Nothing was done; pass --yes to confirm.");
    }
    eprint!("{what}. Type 'yes' to continue: ");
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if answer.trim() != "yes" {
        anyhow::bail!("Aborted, nothing was done.");
    }
    Ok(())
}
