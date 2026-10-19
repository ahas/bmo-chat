//! Each missed-picture reason is its fixed English sentence and its matchable code, and crosses the wire with
//! both; each capability has the words a provider's refusal of it says.

use bmo_chat_client::{ChatCapability, MissedPictureReason};
use serde_json::json;

#[test]
fn every_capability_has_its_words() {
  let words = [
    (ChatCapability::Text, "send a message"),
    (ChatCapability::Photo, "send a photo"),
    (ChatCapability::Album, "send an album of photos"),
    (ChatCapability::Document, "send a file"),
    (ChatCapability::Audio, "send an audio file"),
    (ChatCapability::Voice, "send a voice message"),
    (ChatCapability::Sticker, "send a sticker"),
    (ChatCapability::Reaction, "react to a message"),
    (ChatCapability::Pin, "pin a message"),
    (ChatCapability::Typing, "show a typing indicator"),
    (ChatCapability::Edit, "edit a sent message"),
    (ChatCapability::Delete, "delete a sent message"),
    (ChatCapability::ReplyTo, "reply to a particular message"),
    (ChatCapability::FetchMedia, "fetch a picture or a file later"),
    (ChatCapability::Avatars, "fetch profile pictures"),
    (ChatCapability::History, "read past messages back"),
  ];
  for (capability, said) in words {
    assert_eq!(capability.words(), said);
  }
}

#[test]
fn every_missed_picture_reason() {
  let reasons = [
    (
      MissedPictureReason::NotAnAddress,
      "not-an-address",
      "It is neither a web address nor a file path on this machine.",
    ),
    (
      MissedPictureReason::OutsideDataRoot,
      "outside-data-root",
      "The file is outside the data folder, so it was not sent.",
    ),
    (
      MissedPictureReason::MissingFile,
      "missing-file",
      "There is no such file on this machine.",
    ),
    (MissedPictureReason::Unfetched, "unfetched", "The picture could not be fetched."),
    (
      MissedPictureReason::Platform {
        detail: "Too large.".into(),
      },
      "platform",
      "Too large.",
    ),
  ];
  for (reason, code, sentence) in reasons {
    assert_eq!(reason.code(), code);
    assert_eq!(reason.to_string(), sentence);
    let wire = serde_json::to_value(&reason).unwrap();
    assert_eq!(wire["code"], code);
    assert_eq!(wire["message"], sentence);
    assert_eq!(serde_json::from_value::<MissedPictureReason>(wire).unwrap(), reason);
  }
}

#[test]
fn a_missed_picture_reason_is_read_by_its_fields_not_its_message() {
  let wire = json!({ "code": "unfetched", "message": "anything at all" });
  let reason: MissedPictureReason = serde_json::from_value(wire).unwrap();
  assert_eq!(reason.to_string(), "The picture could not be fetched.");
}
