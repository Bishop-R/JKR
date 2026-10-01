//! The system clipboard, through the desktop's own tools: text out for `viewpos` and `mark`,
//! text in for the console's Ctrl+V. winit has no clipboard and this is a console
//! convenience, not a frame path: a missing tool only means nothing is copied or pasted.
use std::io::Write;
use std::process::{Command, Stdio};

/// Commands that read text to copy from their standard input, tried in order.
const COPY: [&[&str]; 4] = [
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["pbcopy"],
    &["clip"],
];
/// Commands that print the clipboard's text.
const PASTE: [&[&str]; 4] = [
    &["wl-paste", "--no-newline"],
    &["xclip", "-selection", "clipboard", "-o"],
    &["pbpaste"],
    &["powershell", "-NoProfile", "-Command", "Get-Clipboard"],
];

/// Put `text` on the clipboard; `false` if no tool took it.
pub(crate) fn copy(text: &str) -> bool {
    COPY.iter().any(|tool| {
        let Ok(mut child) = Command::new(tool[0])
            .args(&tool[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let written = child
            .stdin
            .take()
            .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
        // The tool keeps serving the selection after it has the text; it is not waited for.
        written
    })
}

/// The clipboard's text, if a tool gave any.
pub(crate) fn paste() -> Option<String> {
    PASTE.iter().find_map(|tool| {
        let output = Command::new(tool[0])
            .args(&tool[1..])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    })
}
