//! Each refusal and each missed-picture reason is its fixed English sentence and its matchable code, and
//! crosses the wire with both.

use bmo_chat::{ChatCapability, ChatRefusal, MissedPicture, MissedPictureReason};
use serde_json::json;

const LABEL: &str = "Platform";

fn label() -> String {
  LABEL.to_owned()
}

#[test]
fn unsupported_capability() {
  let refusal = ChatRefusal::UnsupportedCapability {
    label: label(),
    capability: ChatCapability::Sticker,
  };
  assert_eq!(refusal.code(), "unsupported-capability");
  assert_eq!(refusal.to_string(), "Platform cannot send a sticker.");
  assert_eq!(
    serde_json::to_value(&refusal).unwrap(),
    json!({
      "code": "unsupported-capability", "label": "Platform", "capability": "sticker",
      "message": "Platform cannot send a sticker.",
    })
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
  assert_eq!(refusal.code(), "no-picture-sent");
  let sentence = "Platform: no picture was sent.\n\
    nowhere: It is neither a web address nor a file path on this machine.\n\
    https://example.com/a.png: The platform turned it down.";
  assert_eq!(refusal.to_string(), sentence);
  let wire = json!({
    "code": "no-picture-sent", "label": "Platform", "message": sentence,
    "missed": [
      {
        "address": "nowhere",
        "reason": {
          "code": "not-an-address", "message": "It is neither a web address nor a file path on this machine.",
        },
      },
      {
        "address": "https://example.com/a.png",
        "reason": {
          "code": "platform", "detail": "The platform turned it down.", "message": "The platform turned it down.",
        },
      },
    ],
  });
  assert_eq!(serde_json::to_value(&refusal).unwrap(), wire);
  assert_eq!(serde_json::from_value::<ChatRefusal>(wire).unwrap(), refusal);
}

#[test]
fn speaker_not_allowed() {
  let refusal = ChatRefusal::SpeakerNotAllowed {
    label: label(),
    agent: "agent-2".into(),
  };
  assert_eq!(refusal.code(), "speaker-not-allowed");
  assert_eq!(refusal.to_string(), "Platform cannot send in the name of agent agent-2.");
  assert_eq!(
    serde_json::to_value(&refusal).unwrap(),
    json!({
      "code": "speaker-not-allowed", "label": "Platform", "agent": "agent-2",
      "message": "Platform cannot send in the name of agent agent-2.",
    })
  );
}

#[test]
fn speaker_unnumbered() {
  let refusal = ChatRefusal::SpeakerUnnumbered {
    label: label(),
    agent: "agent-2".into(),
  };
  assert_eq!(refusal.code(), "speaker-unnumbered");
  assert_eq!(
    refusal.to_string(),
    "Platform has no number for agent agent-2, so it cannot send in that agent's name."
  );
  assert_eq!(serde_json::to_value(&refusal).unwrap()["code"], "speaker-unnumbered");
}

#[test]
fn not_ready() {
  let refusal = ChatRefusal::NotReady { label: label() };
  assert_eq!(refusal.code(), "not-ready");
  assert_eq!(refusal.to_string(), "Platform is not ready yet.");
  assert_eq!(
    serde_json::to_value(&refusal).unwrap(),
    json!({ "code": "not-ready", "label": "Platform", "message": "Platform is not ready yet." })
  );
}

#[test]
fn platform() {
  let refusal = ChatRefusal::Platform {
    label: label(),
    detail: "That message is gone.".into(),
  };
  assert_eq!(refusal.code(), "platform");
  assert_eq!(refusal.to_string(), "Platform: That message is gone.");
  assert_eq!(
    serde_json::to_value(&refusal).unwrap(),
    json!({
      "code": "platform", "label": "Platform", "detail": "That message is gone.",
      "message": "Platform: That message is gone.",
    })
  );
}

#[test]
fn every_refusal_crosses_and_keeps_its_label() {
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
      agent: "agent-2".into(),
    },
    ChatRefusal::SpeakerUnnumbered {
      label: label(),
      agent: "agent-3".into(),
    },
    ChatRefusal::NotReady { label: label() },
    ChatRefusal::Platform {
      label: label(),
      detail: "No.".into(),
    },
  ];
  for refusal in refusals {
    assert_eq!(refusal.label(), LABEL);
    let wire = serde_json::to_value(&refusal).unwrap();
    assert_eq!(wire["code"], refusal.code());
    assert_eq!(wire["message"], refusal.to_string());
    assert_eq!(serde_json::from_value::<ChatRefusal>(wire).unwrap(), refusal);
  }
}

#[test]
fn a_refusal_is_read_by_its_fields_not_its_message() {
  let wire = json!({ "code": "not-ready", "label": "Platform", "message": "anything at all" });
  let refusal: ChatRefusal = serde_json::from_value(wire).unwrap();
  assert_eq!(refusal.to_string(), "Platform is not ready yet.");
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
