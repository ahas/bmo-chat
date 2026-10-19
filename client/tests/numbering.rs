//! The checks that keep providers' assigned numbers apart, as `docs/contract.md` states them under
//! "Numbering": a numbering on its own, and two providers' numberings against each other.

use bmo_chat_client::{ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering};

const ROOMS: ChatNumberRange = ChatNumberRange {
  first: -9_000_000,
  last: -8_000_000,
};
const PEOPLE: ChatNumberRange = ChatNumberRange {
  first: 8_000_000,
  last: 9_000_000,
};

fn numbering(rooms: ChatNumberSpace, people: ChatNumberSpace, ranges: ChatNumberRanges) -> ChatNumbering {
  ChatNumbering {
    rooms,
    people,
    messages: ChatNumberSpace::Assigned,
    ranges,
  }
}

fn ranges(rooms: Option<ChatNumberRange>, people: Option<ChatNumberRange>) -> ChatNumberRanges {
  ChatNumberRanges { rooms, people }
}

#[test]
fn a_numbering_whose_assigned_spaces_each_carry_a_range_is_sound() {
  use ChatNumberSpace::{Assigned, Native};
  assert_eq!(numbering(Assigned, Assigned, ranges(Some(ROOMS), Some(PEOPLE))).fault(), None);
  assert_eq!(numbering(Native, ChatNumberSpace::None, ChatNumberRanges::default()).fault(), None);
  let one_number = ChatNumberRange { first: 5, last: 5 };
  assert_eq!(numbering(Assigned, Native, ranges(Some(one_number), None)).fault(), None);
}

#[test]
fn a_missing_misplaced_or_empty_range_is_a_fault() {
  use ChatNumberSpace::{Assigned, Native};
  assert_eq!(
    numbering(Assigned, Native, ChatNumberRanges::default()).fault().as_deref(),
    Some("Its rooms are assigned, and no range is declared for them.")
  );
  assert_eq!(
    numbering(Native, Native, ranges(None, Some(PEOPLE))).fault().as_deref(),
    Some("A range is declared for its people, which it does not assign.")
  );
  let empty = ChatNumberRange { first: 3, last: 2 };
  assert_eq!(
    numbering(Native, Assigned, ranges(None, Some(empty))).fault().as_deref(),
    Some("Its people range runs from 3 down to 2, so it holds no number.")
  );
}

#[test]
fn ranges_of_the_same_space_clash_where_they_share_a_number() {
  use ChatNumberSpace::{Assigned, Native};
  let mine = numbering(Assigned, Assigned, ranges(Some(ROOMS), Some(PEOPLE)));
  let touching = ChatNumberRange {
    first: -8_000_000,
    last: -7_000_000,
  };
  let theirs = numbering(Assigned, Native, ranges(Some(touching), None));
  let clash = Some("Its rooms range, -9000000 to -8000000, overlaps the other's, -8000000 to -7000000.");
  assert_eq!(mine.clash(&theirs).as_deref(), clash);
  assert!(theirs.clash(&mine).is_some(), "a clash is found from either side");

  let beside = ChatNumberRange {
    first: -7_999_999,
    last: -7_000_000,
  };
  let apart = numbering(Assigned, Native, ranges(Some(beside), None));
  assert_eq!(mine.clash(&apart), None);
  let rooms_as_their_people = numbering(Native, Assigned, ranges(None, Some(ROOMS)));
  assert_eq!(mine.clash(&rooms_as_their_people), None, "a rooms range never clashes with people");
}
