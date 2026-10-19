//! Every send verb crosses the wire in the shape `docs/contract.md` and `docs/protocol.md` state, and comes
//! back as it went.

use bmo_chat_client::{
  AlbumItem, ChatSend, ChatSendRequest, ChatSent, ChatSpeaker, ChatVerb, ChatVoice, FileItem,
  MissedPicture, MissedPictureReason,
};
use serde_json::{Value, json};

/// The value encodes to exactly the wire body, and the wire body decodes to exactly the value.
fn crosses<T>(value: &T, wire: Value)
where
  T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
  assert_eq!(serde_json::to_value(value).expect("the value encodes"), wire);
  let text = serde_json::to_string(value).expect("the value encodes as text");
  assert_eq!(&serde_json::from_str::<T>(&text).expect("the text decodes"), value);
  assert_eq!(&serde_json::from_value::<T>(wire).expect("the wire body decodes"), value);
}

fn send(send: ChatSend) -> ChatSendRequest {
  ChatSendRequest::new(7, send)
}

#[test]
fn text_with_every_field() {
  let request = ChatSendRequest {
    voice: Some(ChatVoice::HumanReader),
    speaker: Some(ChatSpeaker {
      agent: "agent-2".into(),
    }),
    tag: Some("tag-1".into()),
    prefix: Some("[agent] ".into()),
    ..send(ChatSend::Text {
      text: "**hello**".into(),
      reply_to: Some(3),
      silent: true,
      fold: true,
    })
  };
  crosses(
    &request,
    json!({
      "verb": "text", "chatId": 7, "text": "**hello**", "replyTo": 3, "silent": true, "fold": true,
      "as": "human-reader", "speaker": { "agent": "agent-2" }, "tag": "tag-1", "prefix": "[agent] ",
    }),
  );
}

#[test]
fn text_with_only_its_words() {
  let request = send(ChatSend::Text {
    text: "hello".into(),
    reply_to: None,
    silent: false,
    fold: false,
  });
  crosses(&request, json!({ "verb": "text", "chatId": 7, "text": "hello" }));
}

#[test]
fn text_carries_no_sanitiser_or_gate_flag() {
  let wire = json!({ "verb": "text", "chatId": 7, "text": "hello", "cleaned": true, "gate": true });
  let request: ChatSendRequest = serde_json::from_value(wire).expect("unknown fields are passed over");
  assert_eq!(
    serde_json::to_value(&request).unwrap(),
    json!({ "verb": "text", "chatId": 7, "text": "hello" })
  );
}

#[test]
fn photo() {
  let request = send(ChatSend::Photo {
    image_url: "https://example.com/a.png".into(),
    caption: Some("a picture".into()),
    reply_to: Some(4),
  });
  crosses(
    &request,
    json!({
      "verb": "photo", "chatId": 7, "imageUrl": "https://example.com/a.png", "caption": "a picture", "replyTo": 4,
    }),
  );
}

#[test]
fn album_of_addresses_and_inline_files() {
  let request = send(ChatSend::Album {
    items: vec![
      AlbumItem::Address {
        address: "https://example.com/a.png".into(),
      },
      AlbumItem::File(FileItem {
        name: "b.png".into(),
        mime: "image/png".into(),
        bytes: vec![1, 2, 3],
      }),
    ],
    caption: Some("two".into()),
    reply_to: Some(5),
  });
  crosses(
    &request,
    json!({
      "verb": "album", "chatId": 7, "caption": "two", "replyTo": 5,
      "items": [
        { "address": "https://example.com/a.png" },
        { "name": "b.png", "mime": "image/png", "bytes": "AQID" },
      ],
    }),
  );
}

#[test]
fn document() {
  let request = send(ChatSend::Document {
    bytes: b"report".to_vec(),
    file_name: "report.txt".into(),
    reply_to: None,
  });
  crosses(
    &request,
    json!({ "verb": "document", "chatId": 7, "bytes": "cmVwb3J0", "fileName": "report.txt" }),
  );
}

#[test]
fn audio() {
  let request = send(ChatSend::Audio {
    audio: vec![0, 255],
    file_name: "song.mp3".into(),
    reply_to: Some(6),
  });
  crosses(
    &request,
    json!({ "verb": "audio", "chatId": 7, "audio": "AP8=", "fileName": "song.mp3", "replyTo": 6 }),
  );
}

#[test]
fn voice() {
  let request = send(ChatSend::Voice {
    audio: vec![9, 8, 7],
    said: "the line".into(),
    caption: Some("under it".into()),
    transcript: Some("what was read".into()),
    reply_to: Some(8),
  });
  crosses(
    &request,
    json!({
      "verb": "voice", "chatId": 7, "audio": "CQgH", "said": "the line", "caption": "under it",
      "transcript": "what was read", "replyTo": 8,
    }),
  );
}

#[test]
fn sticker() {
  crosses(
    &send(ChatSend::Sticker {
      emoji: "🙂".into()
    }),
    json!({ "verb": "sticker", "chatId": 7, "emoji": "🙂" }),
  );
}

#[test]
fn reaction_on_and_off() {
  crosses(
    &send(ChatSend::Reaction {
      message_id: 9,
      emoji: "👍".into(),
    }),
    json!({ "verb": "reaction", "chatId": 7, "messageId": 9, "emoji": "👍" }),
  );
  crosses(
    &send(ChatSend::Reaction {
      message_id: 9,
      emoji: String::new(),
    }),
    json!({ "verb": "reaction", "chatId": 7, "messageId": 9, "emoji": "" }),
  );
}

#[test]
fn pin() {
  crosses(
    &send(ChatSend::Pin {
      message_id: 10,
      on: true,
      silent: true,
    }),
    json!({ "verb": "pin", "chatId": 7, "messageId": 10, "on": true, "silent": true }),
  );
  crosses(
    &send(ChatSend::Pin {
      message_id: 10,
      on: false,
      silent: false,
    }),
    json!({ "verb": "pin", "chatId": 7, "messageId": 10, "on": false }),
  );
}

#[test]
fn typing() {
  crosses(
    &send(ChatSend::Typing { on: true }),
    json!({ "verb": "typing", "chatId": 7, "on": true }),
  );
}

#[test]
fn edit() {
  crosses(
    &send(ChatSend::Edit {
      message_id: 11,
      text: "again".into(),
    }),
    json!({ "verb": "edit", "chatId": 7, "messageId": 11, "text": "again" }),
  );
}

#[test]
fn delete() {
  crosses(
    &send(ChatSend::Delete { message_id: 12 }),
    json!({ "verb": "delete", "chatId": 7, "messageId": 12 }),
  );
}

#[test]
fn every_verb_names_itself() {
  let sends = [
    (
      ChatSend::Text {
        text: String::new(),
        reply_to: None,
        silent: false,
        fold: false,
      },
      ChatVerb::Text,
    ),
    (
      ChatSend::Photo {
        image_url: String::new(),
        caption: None,
        reply_to: None,
      },
      ChatVerb::Photo,
    ),
    (
      ChatSend::Album {
        items: vec![],
        caption: None,
        reply_to: None,
      },
      ChatVerb::Album,
    ),
    (
      ChatSend::Document {
        bytes: vec![],
        file_name: String::new(),
        reply_to: None,
      },
      ChatVerb::Document,
    ),
    (
      ChatSend::Audio {
        audio: vec![],
        file_name: String::new(),
        reply_to: None,
      },
      ChatVerb::Audio,
    ),
    (
      ChatSend::Voice {
        audio: vec![],
        said: String::new(),
        caption: None,
        transcript: None,
        reply_to: None,
      },
      ChatVerb::Voice,
    ),
    (
      ChatSend::Sticker {
        emoji: String::new(),
      },
      ChatVerb::Sticker,
    ),
    (
      ChatSend::Reaction {
        message_id: 1,
        emoji: String::new(),
      },
      ChatVerb::Reaction,
    ),
    (
      ChatSend::Pin {
        message_id: 1,
        on: true,
        silent: false,
      },
      ChatVerb::Pin,
    ),
    (ChatSend::Typing { on: true }, ChatVerb::Typing),
    (
      ChatSend::Edit {
        message_id: 1,
        text: String::new(),
      },
      ChatVerb::Edit,
    ),
    (ChatSend::Delete { message_id: 1 }, ChatVerb::Delete),
  ];
  for (send, verb) in sends {
    assert_eq!(send.verb(), verb);
    let wire = serde_json::to_value(ChatSendRequest::new(1, send)).unwrap();
    assert_eq!(wire["verb"], serde_json::to_value(verb).unwrap());
  }
}

#[test]
fn bytes_that_are_not_base64_are_refused() {
  let wire = json!({ "verb": "document", "chatId": 7, "bytes": "not base64!", "fileName": "a" });
  let why = serde_json::from_value::<ChatSendRequest>(wire).unwrap_err().to_string();
  assert!(why.contains("not base64"), "{why}");
}

#[test]
fn sent() {
  crosses(
    &ChatSent {
      message_id: Some(40),
      missed: vec![],
    },
    json!({ "messageId": 40 }),
  );
  crosses(
    &ChatSent {
      message_id: None,
      missed: vec![],
    },
    json!({ "messageId": null }),
  );
}

#[test]
fn sent_with_missed_pictures() {
  let sent = ChatSent {
    message_id: Some(41),
    missed: vec![MissedPicture {
      address: "/elsewhere/a.png".into(),
      reason: MissedPictureReason::OutsideDataRoot,
    }],
  };
  crosses(
    &sent,
    json!({
      "messageId": 41,
      "missed": [{
        "address": "/elsewhere/a.png",
        "reason": {
          "code": "outside-data-root", "message": "The file is outside the data folder, so it was not sent.",
        },
      }],
    }),
  );
}
