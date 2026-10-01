//! Tab completion of the command or cvar name being typed (`Field_CompleteCommand`,
//! `qcommon/common.cpp`): a unique name completes with a trailing space; several
//! complete to their longest shared prefix and are listed in the scrollback, with
//! descriptions while the listing is short.

use super::{ConsoleLineKind, Shell, ascii_starts_with_ignore_case, builtin_commands};

/// Longest cvar value shown in a completion listing, as `TRUNCATE_LENGTH`.
const LISTED_VALUE_CHARS: usize = 64;
/// Most matches listed with their descriptions. Longer listings drop them and end with
/// the match count, so one listing fills as little of the bounded scrollback as it can.
const DESCRIBED_MATCHES: usize = 16;

/// The key that asked for a completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionKey {
    /// Tab: candidates are listed whenever more than one remains.
    Tab,
    /// Enter with `cl_allowEnterCompletion`: the line runs next, so candidates are
    /// listed only when the completed text is not itself a full name.
    Enter,
}

/// One completion candidate.
struct Candidate<'a> {
    name: &'a str,
    description: &'a str,
    /// Current value of a cvar; `None` for a command.
    value: Option<String>,
}

impl<'a> Candidate<'a> {
    fn command(name: &'a str, description: &'a str) -> Self {
        Self {
            name,
            description,
            value: None,
        }
    }
}

impl Shell {
    /// Complete the name at the end of `line`: the first word after its last unquoted
    /// `;`, without a leading `/` or `\`. Returns the completed line, or `None` when
    /// nothing matches or the line already continues past the name.
    pub fn complete_line(&mut self, line: &str, key: CompletionKey) -> Option<String> {
        let start = name_start(line)?;
        let (name, listing) = self.completion(&line[start..], key)?;
        let completed = format!("{}{name}", &line[..start]);
        if !listing.is_empty() {
            self.push_line(ConsoleLineKind::Input, format!("] {completed}"));
            for text in listing {
                self.push_line(ConsoleLineKind::Output, text);
            }
        }
        Some(completed)
    }

    /// The unique name `line` would complete to, for a prompt hint; does not allocate.
    pub fn completion_hint(&self, line: &str) -> Option<&str> {
        self.unique_command_completion(&line[name_start(line)?..])
    }

    /// The completed name and, when it is not unique, the lines listing the candidates.
    fn completion(&self, prefix: &str, key: CompletionKey) -> Option<(String, Vec<String>)> {
        let candidates = self.candidates(prefix);
        if let [only] = candidates.as_slice() {
            return Some((format!("{} ", only.name), Vec::new()));
        }
        let shared = shared_prefix(candidates.iter().map(|candidate| candidate.name))?;
        let full_name = candidates
            .iter()
            .any(|candidate| candidate.name.len() == shared.len());
        if key == CompletionKey::Enter && full_name {
            return Some((shared.to_owned(), Vec::new()));
        }
        let described = candidates.len() <= DESCRIBED_MATCHES;
        let mut listing = Vec::with_capacity(candidates.len() * 2 + 1);
        for candidate in &candidates {
            listing.push(match &candidate.value {
                None => format!("Cmd   {}", candidate.name),
                Some(value) => format!("Cvar  {} = \"{value}\"", candidate.name),
            });
            if described && !candidate.description.is_empty() {
                listing.push(format!("      ^2{}", candidate.description));
            }
        }
        if !described {
            listing.push(format!(
                "^3{} matches; type more of the name to narrow them",
                candidates.len()
            ));
        }
        Some((shared.to_owned(), listing))
    }

    /// Commands, then cvars, whose names start with `prefix`, each sorted; a name that
    /// is both a command and a cvar is listed once, as the command.
    fn candidates(&self, prefix: &str) -> Vec<Candidate<'_>> {
        let mut commands = builtin_commands()
            .map(|(name, description)| Candidate::command(name, description))
            .chain(
                self.commands
                    .iter()
                    .map(|(name, description)| Candidate::command(name, description)),
            )
            .chain(
                self.external_command_help
                    .iter()
                    .map(|(name, description)| Candidate::command(name, description)),
            )
            .filter(|candidate| ascii_starts_with_ignore_case(candidate.name, prefix))
            .collect::<Vec<_>>();
        commands.sort_by_cached_key(|candidate| candidate.name.to_ascii_lowercase());
        commands.dedup_by(|later, earlier| later.name.eq_ignore_ascii_case(earlier.name));
        let mut cvars = self
            .cvars
            .iter()
            .filter(|cvar| ascii_starts_with_ignore_case(&cvar.name, prefix))
            .filter(|cvar| {
                !commands
                    .iter()
                    .any(|command| command.name.eq_ignore_ascii_case(&cvar.name))
            })
            .map(|cvar| Candidate {
                name: &cvar.name,
                description: &cvar.description,
                value: Some(listed_value(cvar.value.as_text())),
            })
            .collect::<Vec<_>>();
        cvars.sort_by_cached_key(|candidate| candidate.name.to_ascii_lowercase());
        commands.append(&mut cvars);
        commands
    }
}

/// Byte offset of the name being typed: after the last `;` outside quotes, leading
/// whitespace and one `/` or `\`. `None` when that name is empty or an argument follows it.
fn name_start(line: &str) -> Option<usize> {
    let mut segment = 0;
    let mut quoted = false;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b';' if !quoted => segment = index + 1,
            _ => {}
        }
    }
    let command = line[segment..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    let name = command.strip_prefix(['/', '\\']).unwrap_or(command);
    let continues = name
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || byte == b'"');
    (!name.is_empty() && !continues).then(|| line.len() - name.len())
}

/// Longest case-insensitive prefix shared by every name, spelled as the first.
fn shared_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let first = names.next()?;
    let mut length = first.len();
    for name in names {
        length = first
            .bytes()
            .zip(name.bytes())
            .take(length)
            .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
            .count();
    }
    while !first.is_char_boundary(length) {
        length -= 1;
    }
    Some(&first[..length])
}

fn listed_value(value: String) -> String {
    match value.char_indices().nth(LISTED_VALUE_CHARS) {
        Some((end, _)) => format!("{}...", &value[..end]),
        None => value,
    }
}
