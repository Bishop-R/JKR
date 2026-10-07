//! jaPRO's `ignore` (`sess.ignore`, `ClientIsIgnored`, `ClientIgnore`, `ClientIgnoreAll`):
//! whose chat a client no longer receives. jaPRO keeps a 32-bit mask of client numbers;
//! here it is everyone-or-nobody with the clients toggled against it, which takes any
//! number of clients.

use std::collections::BTreeSet;

/// One client's ignore list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ignore {
    /// Everyone ignored, but the toggled.
    all: bool,
    /// The clients whose state differs from `all`.
    toggled: BTreeSet<u16>,
}

impl Ignore {
    /// `ClientIsIgnored`: whether `speaker`'s chat is kept from `listener` (never its
    /// own).
    pub fn ignores(&self, listener: usize, speaker: usize) -> bool {
        listener != speaker && self.all != self.toggled.contains(&(speaker as u16))
    }

    /// `ClientIgnore`: `target` ignored, or no longer; whether it now is.
    pub fn toggle(&mut self, target: usize) -> bool {
        let target = target as u16;
        if !self.toggled.remove(&target) {
            self.toggled.insert(target);
        }
        self.all != self.toggled.contains(&target)
    }

    /// `ClientIgnoreAll`: everyone ignored, unless everyone already is — then nobody;
    /// whether everyone now is.
    pub fn toggle_all(&mut self) -> bool {
        let everyone = self.all && self.toggled.is_empty();
        self.all = !everyone;
        self.toggled.clear();
        self.all
    }

    /// A client gone: whoever takes its place next starts as everyone else does (jaPRO's
    /// mask kept the bit for the next one).
    pub fn forget(&mut self, client: usize) {
        self.toggled.remove(&(client as u16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignoring_one_everyone_and_all_but_one() {
        let mut ignore = Ignore::default();
        assert!(!ignore.ignores(0, 3));
        assert!(ignore.toggle(3));
        assert!(ignore.ignores(0, 3) && !ignore.ignores(0, 4));
        assert!(ignore.toggle_all());
        assert!(ignore.ignores(0, 4) && !ignore.ignores(0, 0));
        assert!(!ignore.toggle(4));
        assert!(!ignore.ignores(0, 4) && ignore.ignores(0, 5));
        // Not everyone any more: everyone again, then nobody.
        assert!(ignore.toggle_all());
        assert!(!ignore.toggle_all());
        assert!(!ignore.ignores(0, 5));
        ignore.toggle(6);
        ignore.forget(6);
        assert!(!ignore.ignores(0, 6));
    }
}
