//! Which order to put the call list in.
//!
//! # What this file is here to teach
//!
//! Why letting a caller choose an order has to be an **allow-list**, and never a
//! string. The obvious way to write this is to take the column name off the request
//! and paste it into the query — and that is how somebody ends up asking to sort by
//! `name; DROP TABLE lead`. An enum cannot hold anything we did not write down, so
//! the dangerous version is not merely discouraged, it is unrepresentable.

use serde::{Deserialize, Serialize};

/// A column to order by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    Name,
    Trade,
    Phone,
    /// Why they qualified — no website, social only, broken, insecure.
    Why,
    Certificate,
    Hours,
    Reviews,
    /// How far from the centre of its own city, which is where you are.
    ///
    /// The default, and the one opinion the list rests on: a morning of calling is a
    /// morning of driving, so the shop round the corner comes before the one across
    /// the city.
    #[default]
    Distance,
    FirstSeen,
    /// Where you have got to with them.
    Status,
}

impl Sort {
    /// The word this goes by in a web address.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Trade => "trade",
            Self::Phone => "phone",
            Self::Why => "why",
            Self::Certificate => "certificate",
            Self::Hours => "hours",
            Self::Reviews => "reviews",
            Self::Distance => "distance",
            Self::FirstSeen => "first_seen",
            Self::Status => "status",
        }
    }

    /// The heading a person reads.
    pub fn heading(self) -> &'static str {
        match self {
            Self::Name => "Business",
            Self::Trade => "Trade",
            Self::Phone => "Telephone",
            Self::Why => "Why",
            // Short on purpose. The headings are set in spaced capitals, so a long
            // one sets the whole column's width and squeezes the rows beside it —
            // which is how "1.1 km" ended up on two lines with space to spare.
            Self::Certificate => "Cert.",
            Self::Hours => "Hours",
            Self::Reviews => "Reviews",
            Self::Distance => "Away",
            Self::FirstSeen => "Seen",
            Self::Status => "State",
        }
    }

    /// Every column you can sort by, in the order they appear on screen.
    pub const ALL: [Sort; 10] = [
        Sort::Name,
        Sort::Trade,
        Sort::Phone,
        Sort::Why,
        Sort::Certificate,
        Sort::Hours,
        Sort::Reviews,
        Sort::Distance,
        Sort::FirstSeen,
        Sort::Status,
    ];

    /// Which way round this column reads best the first time you click it.
    ///
    /// A number wants its biggest first — you click "Reviews" to see the busiest,
    /// not the quietest. A word wants A to Z. Getting this wrong means every column
    /// needs two clicks to be useful.
    pub fn biggest_first(self) -> bool {
        matches!(self, Self::Reviews | Self::FirstSeen)
    }
}

/// Read a column name off a request.
///
/// Anything we do not recognise comes back as `None`, and the caller uses the default
/// order. Refusing the whole request would be worse: the page builds these from its
/// own headings, so an unknown one means a bug here, not a caller doing something odd.
impl std::str::FromStr for Sort {
    type Err = UnknownSort;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Sort::ALL
            .into_iter()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| UnknownSort(text.to_owned()))
    }
}

/// What comes back when somebody asks for a column that does not exist.
#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a column you can sort by")]
pub struct UnknownSort(pub String);

#[cfg(test)]
#[path = "sort_tests.rs"]
mod tests;
