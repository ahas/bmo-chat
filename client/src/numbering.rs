//! Whose numbers a provider's rooms, people and messages carry, the ranges it assigns them in, and the checks
//! that keep two providers' ranges apart.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whose numbers one kind of thing carries on a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatNumberSpace {
  /// The platform's own numbers, used as they are.
  Native,
  /// Numbers the provider gave out because the platform's cannot be used.
  Assigned,
  /// None of its own: the provider speaks into rooms and people another provider numbers.
  None,
}

/// The numbers a provider assigns in one space: from `first` to `last`, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ChatNumberRange {
  pub first: i64,
  pub last: i64,
}

impl ChatNumberRange {
  /// Whether the two ranges share a number.
  pub fn overlaps(&self, other: &Self) -> bool {
    self.first <= other.last && other.first <= self.last
  }
}

/// The range of each space a provider assigns numbers in; null for a space it does not assign. Messages carry
/// none, since a message is numbered within its room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChatNumberRanges {
  pub rooms: Option<ChatNumberRange>,
  pub people: Option<ChatNumberRange>,
}

/// One number space for each of rooms, people and messages, and the ranges of the spaces assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ChatNumbering {
  pub rooms: ChatNumberSpace,
  pub people: ChatNumberSpace,
  pub messages: ChatNumberSpace,
  pub ranges: ChatNumberRanges,
}

impl ChatNumbering {
  /// The spaces that carry a range, each named, with its space and its range.
  fn ranged(&self) -> [(&'static str, ChatNumberSpace, Option<ChatNumberRange>); 2] {
    [("rooms", self.rooms, self.ranges.rooms), ("people", self.people, self.ranges.people)]
  }

  /// Why this numbering cannot be taken in on its own, in English, or null where it can: an `assigned` space
  /// without a range, a range for a space that is not `assigned`, or a range whose `first` is above its
  /// `last`.
  pub fn fault(&self) -> Option<String> {
    self.ranged().into_iter().find_map(|(name, space, range)| match (space, range) {
      (ChatNumberSpace::Assigned, None) => {
        Some(format!("Its {name} are assigned, and no range is declared for them."))
      }
      (ChatNumberSpace::Assigned, Some(range)) if range.first > range.last => Some(format!(
        "Its {name} range runs from {} down to {}, so it holds no number.",
        range.first, range.last
      )),
      (ChatNumberSpace::Native | ChatNumberSpace::None, Some(_)) => {
        Some(format!("A range is declared for its {name}, which it does not assign."))
      }
      _ => None,
    })
  }

  /// Why this numbering and another provider's cannot both be taken in, in English, or null where they can:
  /// a range of one space that overlaps the other's range of the same space.
  pub fn clash(&self, other: &Self) -> Option<String> {
    self
      .ranged()
      .into_iter()
      .zip(other.ranged())
      .find_map(|((name, _, mine), (_, _, theirs))| {
        let (mine, theirs) = (mine?, theirs?);
        mine.overlaps(&theirs).then(|| {
          format!(
            "Its {name} range, {} to {}, overlaps the other's, {} to {}.",
            mine.first, mine.last, theirs.first, theirs.last
          )
        })
      })
  }
}
