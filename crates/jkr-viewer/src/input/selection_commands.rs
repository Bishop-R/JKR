//! Completion/help metadata for locally dispatched selection commands.

/// Register metadata; execution is intercepted by GameplayInput before shell fallback.
pub(super) fn register(shell: &mut jkr_shell::Shell) -> Result<(), jkr_shell::CommandError> {
    for (name, help) in [
        (
            "forcenext",
            "Select next usable Force power (Use held: inventory)",
        ),
        (
            "forceprev",
            "Select previous usable Force power (Use held: inventory)",
        ),
        ("invnext", "Select next inventory item"),
        ("invprev", "Select previous inventory item"),
    ] {
        shell.commands.register(name, help, |_| {
            Err(jkr_shell::CommandError::Handler(
                "Viewer input dispatcher required".into(),
            ))
        })?;
    }
    Ok(())
}
