use std::{collections::HashSet, fmt};

#[derive(Clone, Debug, Default)]
pub struct Flags(HashSet<Flag>);

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags: Vec<_> = self.0.clone().into_iter().map(|f| f.to_string()).collect();
        flags.sort();
        write!(f, "{}", flags.join(""))
    }
}

impl<T: IntoIterator<Item = Flag>> From<T> for Flags {
    fn from(flags: T) -> Self {
        Flags(flags.into_iter().collect())
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
