use std::{collections::HashSet, fmt};

use log::debug;

#[derive(Clone, Debug, Default)]
pub struct Flags(HashSet<Flag>);

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags: Vec<_> = self.0.clone().into_iter().map(|f| f.to_string()).collect();
        flags.sort();
        write!(f, "{}", flags.join(""))
    }
}

impl Flags {
    pub fn from_iter(iter: impl IntoIterator<Item = Flag>) -> Self {
        Flags(iter.into_iter().collect())
    }

    pub fn extend(&mut self, flags: Flags) {
        self.0.extend(flags.0)
    }

    pub fn difference(&mut self, flags: &Flags) {
        self.0 = self.0.difference(&flags.0).cloned().collect();
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum Flag {
    Passed,
    Replied,
    Seen,
    Trashed,
    Draft,
    Flagged,
}

impl Flag {
    pub fn from_char(c: char) -> Option<Flag> {
        match c {
            'P' => Some(Flag::Passed),
            'R' => Some(Flag::Replied),
            'S' => Some(Flag::Seen),
            'T' => Some(Flag::Trashed),
            'D' => Some(Flag::Draft),
            'F' => Some(Flag::Flagged),
            c => {
                debug!("invalid maildir flag `{c}`, ignoring");
                None
            }
        }
    }
}

impl fmt::Display for Flag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => write!(f, "P"),
            Self::Replied => write!(f, "R"),
            Self::Seen => write!(f, "S"),
            Self::Trashed => write!(f, "T"),
            Self::Draft => write!(f, "D"),
            Self::Flagged => write!(f, "F"),
        }
    }
}
