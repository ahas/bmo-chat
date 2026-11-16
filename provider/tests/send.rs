//! Every kind of send crosses the wire in the shape `docs/contract.md` and `docs/protocol.md` state, and
//! comes back as it went.

use bmo_chat::{
  AlbumItem, ChatSend, ChatSendRequest, ChatSent, ChatSticker, ChatVerb, ChatVoice, FileItem,
  MissedPicture, MissedPictureReason, StickerItem,
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

/// The same send, answering a message.
fn answering(message_id: i64, send: ChatSend) -> ChatSendRequest {
  ChatSendRequest {
    reply_to_message_id: Some(message_id),
    ..ChatSendRequest::new(7, send)
  }
}

#[test]
fn text_with_every_field() {
  let request = ChatSendRequest {
    mode: Some(ChatVoice::HumanReader),
    speaker: Some("helper-2".into()),
    tag: Some("tag-1".into()),
    prefix: Some("[helper] ".into()),
    reply_to_message_id: Some(3),
    ..send(ChatSend::Text {
      text: "**hello**".into(),
      silent: true,
      fold: true,
    })
  };
  crosses(
    &request,
    json!({
      "kind": "text", "roomId": 7, "fields": { "text": "**hello**", "silent": true, "fold": true },
      "mode": "human-reader", "speaker": "helper-2", "tag": "tag-1", "prefix": "[helper] ",
      "replyToMessageId": 3,
    }),
  );
}

#[test]
fn text_with_only_its_words() {
  let request = send(ChatSend::Text {
    text: "hello".into(),
    silent: false,
    fold: false,
  });
  crosses(&request, json!({ "kind": "text", "roomId": 7, "fields": { "text": "hello" } }));
}

#[test]
fn text_carries_no_flag_the_protocol_does_not_state() {
  let wire = json!({
    "kind": "text", "roomId": 7, "fields": { "text": "hello", "cleaned": true }, "checked": true,
  });
  let request: ChatSendRequest = serde_json::from_value(wire).expect("unknown fields are passed over");
  assert_eq!(
    serde_json::to_value(&request).unwrap(),
    json!({ "kind": "text", "roomId": 7, "fields": { "text": "hello" } })
  );
}

#[test]
fn photo() {
  let request = answering(
    4,
    ChatSend::Photo {
      image_url: "https://example.com/a.png".into(),
      caption: Some("a picture".into()),
    },
  );
  crosses(
    &request,
    json!({
      "kind": "photo", "roomId": 7, "replyToMessageId": 4,
      "fields": { "imageUrl": "https://example.com/a.png", "caption": "a picture" },
    }),
  );
}

#[test]
fn album_of_addresses_and_inline_files() {
  let request = answering(
    5,
    ChatSend::Album {
      items: vec![
        AlbumItem::Address {
          address: "https://example.com/a.png".into(),
        },
        AlbumItem::File(FileItem {
          name: "b.png".into(),
          mime: "image/png".into(),
          bytes: vec![1, 2, 3].into(),
        }),
      ],
      caption: Some("two".into()),
    },
  );
  crosses(
    &request,
    json!({
      "kind": "album", "roomId": 7, "replyToMessageId": 5,
      "fields": {
        "caption": "two",
        "items": [
          { "address": "https://example.com/a.png" },
          { "name": "b.png", "mime": "image/png", "bytes": "AQID" },
        ],
      },
    }),
  );
}

#[test]
fn document() {
  let request = send(ChatSend::Document {
    bytes: b"report".to_vec().into(),
    file_name: "report.txt".into(),
  });
  crosses(
    &request,
    json!({ "kind": "document", "roomId": 7, "fields": { "bytes": "cmVwb3J0", "fileName": "report.txt" } }),
  );
}

#[test]
fn audio() {
  let request = answering(
    6,
    ChatSend::Audio {
      audio: vec![0, 255].into(),
      file_name: "song.mp3".into(),
    },
  );
  crosses(
    &request,
    json!({
      "kind": "audio", "roomId": 7, "replyToMessageId": 6,
      "fields": { "audio": "AP8=", "fileName": "song.mp3" },
    }),
  );
}

#[test]
fn voice() {
  let request = answering(
    8,
    ChatSend::Voice {
      audio: vec![9, 8, 7].into(),
      said: "the line".into(),
      caption: Some("under it".into()),
      transcript: Some("what was read".into()),
    },
  );
  crosses(
    &request,
    json!({
      "kind": "voice", "roomId": 7, "replyToMessageId": 8,
      "fields": { "audio": "CQgH", "said": "the line", "caption": "under it", "transcript": "what was read" },
    }),
  );
}

#[test]
fn sticker() {
  crosses(
    &send(ChatSend::Sticker(ChatSticker::Emoji {
      emoji: "🙂".into()
    })),
    json!({ "kind": "sticker", "roomId": 7, "fields": { "emoji": "🙂" } }),
  );
}

#[test]
fn sticker_item() {
  let item = StickerItem {
    id: "4412.emot_002.webp".into(),
    pack: Some("4412".into()),
    title: Some("Waving cat".into()),
    image_url: Some("https://example.com/4412/emot_002.webp".into()),
    thumbnail_url: Some("https://example.com/4412/emot_002_thumb.webp".into()),
  };
  crosses(
    &send(ChatSend::Sticker(ChatSticker::Item { item })),
    json!({ "kind": "sticker", "roomId": 7, "fields": { "item": {
      "id": "4412.emot_002.webp",
      "pack": "4412",
      "title": "Waving cat",
      "imageUrl": "https://example.com/4412/emot_002.webp",
      "thumbnailUrl": "https://example.com/4412/emot_002_thumb.webp",
    } } }),
  );
}

#[test]
fn sticker_item_with_its_key_alone() {
  let item = StickerItem {
    id: "CAACAgIAAxkBAAE".into(),
    pack: None,
    title: None,
    image_url: None,
    thumbnail_url: None,
  };
  crosses(
    &send(ChatSend::Sticker(ChatSticker::Item { item })),
    json!({ "kind": "sticker", "roomId": 7, "fields": { "item": { "id": "CAACAgIAAxkBAAE" } } }),
  );
}

#[test]
fn reaction_on_and_off() {
  crosses(
    &send(ChatSend::Reaction {
      message_id: 9,
      emoji: "👍".into(),
    }),
    json!({ "kind": "reaction", "roomId": 7, "fields": { "messageId": 9, "emoji": "👍" } }),
  );
  crosses(
    &send(ChatSend::Reaction {
      message_id: 9,
      emoji: String::new(),
    }),
    json!({ "kind": "reaction", "roomId": 7, "fields": { "messageId": 9, "emoji": "" } }),
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
    json!({ "kind": "pin", "roomId": 7, "fields": { "messageId": 10, "on": true, "silent": true } }),
  );
  crosses(
    &send(ChatSend::Pin {
      message_id: 10,
      on: false,
      silent: false,
    }),
    json!({ "kind": "pin", "roomId": 7, "fields": { "messageId": 10, "on": false } }),
  );
}

#[test]
fn typing() {
  crosses(
    &send(ChatSend::Typing { on: true }),
    json!({ "kind": "typing", "roomId": 7, "fields": { "on": true } }),
  );
}

#[test]
fn edit() {
  crosses(
    &send(ChatSend::Edit {
      message_id: 11,
      text: "again".into(),
    }),
    json!({ "kind": "edit", "roomId": 7, "fields": { "messageId": 11, "text": "again" } }),
  );
}

#[test]
fn delete() {
  crosses(
    &send(ChatSend::Delete { message_id: 12 }),
    json!({ "kind": "delete", "roomId": 7, "fields": { "messageId": 12 } }),
  );
}

#[test]
fn every_kind_names_itself() {
  let sends = [
    (
      ChatSend::Text {
        text: String::new(),
        silent: false,
        fold: false,
      },
      ChatVerb::Text,
    ),
    (
      ChatSend::Photo {
        image_url: String::new(),
        caption: None,
      },
      ChatVerb::Photo,
    ),
    (
      ChatSend::Album {
        items: vec![],
        caption: None,
      },
      ChatVerb::Album,
    ),
    (
      ChatSend::Document {
        bytes: vec![].into(),
        file_name: String::new(),
      },
      ChatVerb::Document,
    ),
    (
      ChatSend::Audio {
        audio: vec![].into(),
        file_name: String::new(),
      },
      ChatVerb::Audio,
    ),
    (
      ChatSend::Voice {
        audio: vec![].into(),
        said: String::new(),
        caption: None,
        transcript: None,
      },
      ChatVerb::Voice,
    ),
    (
      ChatSend::Sticker(ChatSticker::Emoji {
        emoji: String::new(),
      }),
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
  for (send, kind) in sends {
    assert_eq!(send.kind(), kind);
    let wire = serde_json::to_value(ChatSendRequest::new(1, send)).unwrap();
    assert_eq!(wire["kind"], serde_json::to_value(kind).unwrap());
  }
}

#[test]
fn bytes_that_are_not_base64_are_refused() {
  let wire = json!({
    "kind": "document", "roomId": 7, "fields": { "bytes": "not base64!", "fileName": "a" },
  });
  let why = serde_json::from_value::<ChatSendRequest>(wire).unwrap_err().to_string();
  assert!(why.contains("not base64"), "{why}");
}

#[test]
fn a_kind_without_its_fields_is_refused() {
  let wire = json!({ "kind": "text", "roomId": 7 });
  assert!(serde_json::from_value::<ChatSendRequest>(wire).is_err());
}

#[test]
fn sent() {
  crosses(
    &ChatSent {
      message_id: Some(40),
      missed: vec![],
    },
    json!({ "messageId": 40, "missed": [] }),
  );
  crosses(
    &ChatSent {
      message_id: None,
      missed: vec![],
    },
    json!({ "messageId": null, "missed": [] }),
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
