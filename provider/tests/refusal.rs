//! Each refusal is its fixed English sentence and its matchable code, and crosses the wire in the error shape
//! as both; each missed-picture reason crosses with its sentence and its code.

use bmo_chat::{ChatCapability, ChatRefusal, ErrorCode, MissedPicture, MissedPictureReason, WireError};
use serde_json::json;

const LABEL: &str = "Platform";

fn label() -> String {
  LABEL.to_owned()
}

/// The refusal as it crosses: its code and its sentence, and no other key.
fn crossed(refusal: &ChatRefusal) -> serde_json::Value {
  serde_json::to_value(WireError::from(refusal.clone())).unwrap()
}

#[test]
fn unsupported_capability() {
  let refusal = ChatRefusal::UnsupportedCapability {
    label: label(),
    capability: ChatCapability::Sticker,
  };
  assert_eq!(refusal.code(), ErrorCode::UnsupportedCapability);
  assert_eq!(refusal.to_string(), "Platform cannot send a sticker.");
  assert_eq!(
    crossed(&refusal),
    json!({ "code": "unsupported-capability", "error": "Platform cannot send a sticker." })
  );
}

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
    let refusal = ChatRefusal::UnsupportedCapability {
      label: label(),
      capability,
    };
    assert_eq!(refusal.to_string(), format!("Platform cannot {said}."));
  }
}

#[test]
fn no_picture_sent() {
  let refusal = ChatRefusal::NoPictureSent {
    label: label(),
    missed: vec![
      MissedPicture {
        address: "nowhere".into(),
        reason: MissedPictureReason::NotAnAddress,
      },
      MissedPicture {
        address: "https://example.com/a.png".into(),
        reason: MissedPictureReason::Platform {
          detail: "The platform turned it down.".into(),
        },
      },
    ],
  };
  assert_eq!(refusal.code(), ErrorCode::NoPictureSent);
  let sentence = "Platform: no picture was sent.\n\
    nowhere: It is neither a web address nor a file path on this machine.\n\
    https://example.com/a.png: The platform turned it down.";
  assert_eq!(refusal.to_string(), sentence);
  assert_eq!(crossed(&refusal), json!({ "code": "no-picture-sent", "error": sentence }));
}

#[test]
fn speaker_not_allowed() {
  let refusal = ChatRefusal::SpeakerNotAllowed {
    label: label(),
    speaker: "helper-2".into(),
  };
  assert_eq!(refusal.code(), ErrorCode::SpeakerNotAllowed);
  assert_eq!(refusal.to_string(), "Platform cannot send in the name of helper-2.");
}

#[test]
fn speaker_unnumbered() {
  let refusal = ChatRefusal::SpeakerUnnumbered {
    label: label(),
    speaker: "helper-2".into(),
  };
  assert_eq!(refusal.code(), ErrorCode::SpeakerUnnumbered);
  assert_eq!(
    refusal.to_string(),
    "Platform has no number for helper-2, so it cannot send in that name."
  );
}

#[test]
fn not_ready() {
  let refusal = ChatRefusal::NotReady { label: label() };
  assert_eq!(refusal.code(), ErrorCode::NotReady);
  assert_eq!(refusal.to_string(), "Platform is not ready yet.");
}

#[test]
fn platform() {
  let refusal = ChatRefusal::Platform {
    label: label(),
    detail: "That message cannot be pinned.".into(),
  };
  assert_eq!(refusal.code(), ErrorCode::Platform);
  assert_eq!(refusal.to_string(), "Platform: That message cannot be pinned.");
}

#[test]
fn gone() {
  let refusal = ChatRefusal::Gone {
    label: label(),
    detail: "That message is gone.".into(),
  };
  assert_eq!(refusal.code(), ErrorCode::Gone);
  assert_eq!(refusal.to_string(), "Platform: That message is gone.");
  assert_eq!(crossed(&refusal), json!({ "code": "gone", "error": "Platform: That message is gone." }));
}

#[test]
fn every_refusal_crosses_as_its_code_and_sentence_and_keeps_its_label() {
  let refusals = [
    ChatRefusal::UnsupportedCapability {
      label: label(),
      capability: ChatCapability::FetchMedia,
    },
    ChatRefusal::NoPictureSent {
      label: label(),
      missed: vec![MissedPicture {
        address: "a".into(),
        reason: MissedPictureReason::Unfetched,
      }],
    },
    ChatRefusal::SpeakerNotAllowed {
      label: label(),
      speaker: "helper-2".into(),
    },
    ChatRefusal::SpeakerUnnumbered {
      label: label(),
      speaker: "helper-3".into(),
    },
    ChatRefusal::NotReady { label: label() },
    ChatRefusal::Platform {
      label: label(),
      detail: "No.".into(),
    },
    ChatRefusal::Gone {
      label: label(),
      detail: "No more.".into(),
    },
  ];
  for refusal in refusals {
    assert_eq!(refusal.label(), LABEL);
    assert_eq!(WireError::from(refusal.clone()), WireError::new(refusal.code(), refusal.to_string()));
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
