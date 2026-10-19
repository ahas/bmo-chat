//! The pieces beside the sends cross the wire in the shape `docs/contract.md` and `docs/protocol.md` state,
//! and come back as they went.

use std::collections::BTreeSet;

use bmo_chat_client::{
  AvatarList, AvatarsUpdated, ChatAvatar, ChatAvatarKind, ChatCapability, ChatDeclaration,
  ChatDialectDeclaration, ChatDialectLimits, ChatFileKind, ChatGathered, ChatHandover, ChatMarkup,
  ChatMediaRequest, ChatMessage, ChatModeStanding, ChatNumberRange, ChatNumberRanges, ChatNumberSpace,
  ChatNumbering, ChatReach, ChatReadMode, ChatVerb, ChatVoice, ErrorCode, GatheredOutcome, HistoryKept,
  HistoryRoom, InboundChatContext, Listened, ListenedQuery, MessageFile, RoomFacts, RoomKind, RoomsOf,
  RoomsOfQuery, WireError,
};
use serde_json::{Value, json};

fn crosses<T>(value: &T, wire: Value)
where
  T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
  assert_eq!(serde_json::to_value(value).expect("the value encodes"), wire);
  assert_eq!(&serde_json::from_value::<T>(wire).expect("the wire body decodes"), value);
}

fn message() -> ChatMessage {
  ChatMessage {
    chat_id: -100,
    message_id: 5,
    sender_id: Some(42),
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000_000,
    text: "hello".into(),
    reply_to_message_id: None,
    via: "platform".into(),
    album: None,
    mode: ChatVoice::BotAccount,
  }
}

fn message_wire() -> Value {
  json!({
    "chatId": -100, "messageId": 5, "senderId": 42, "senderName": "A Person", "sentAt": 1_760_000_000_000_i64,
    "text": "hello", "replyToMessageId": null, "via": "platform", "album": null, "mode": "bot-account",
  })
}

#[test]
fn every_enum_member_by_its_wire_name() {
  fn names<T: serde::Serialize>(members: &[T]) -> Vec<String> {
    members
      .iter()
      .map(|member| serde_json::to_value(member).unwrap().as_str().unwrap().to_owned())
      .collect()
  }
  use ChatVoice::*;
  assert_eq!(
    names(&[BotAccount, HumanReader, HumanAvatar, HumanAsAgent, Direct]),
    ["bot-account", "human-reader", "human-avatar", "human-as-agent", "direct"]
  );
  assert_eq!(
    names(&[ChatReach::AllMessages, ChatReach::MentionsOnly]),
    ["all-messages", "mentions-only"]
  );
  assert_eq!(
    names(&[ChatModeStanding::Active, ChatModeStanding::Inactive, ChatModeStanding::Unsupported]),
    ["active", "inactive", "unsupported"]
  );
  assert_eq!(
    names(&[ChatNumberSpace::Native, ChatNumberSpace::Assigned, ChatNumberSpace::None]),
    ["native", "assigned", "none"]
  );
  assert_eq!(names(&[RoomKind::Owner, RoomKind::Group]), ["owner", "group"]);
  assert_eq!(names(&[ChatAvatarKind::Room, ChatAvatarKind::Person]), ["room", "person"]);
  use ChatFileKind as F;
  assert_eq!(
    names(&[
      F::Photo,
      F::Video,
      F::RoundVideo,
      F::Animation,
      F::Audio,
      F::VoiceNote,
      F::Sticker,
      F::Document
    ]),
    [
      "photo",
      "video",
      "round-video",
      "animation",
      "audio",
      "voice-note",
      "sticker",
      "document"
    ]
  );
  use ChatVerb as V;
  let verbs = [
    V::Text,
    V::Photo,
    V::Album,
    V::Document,
    V::Audio,
    V::Voice,
    V::Sticker,
    V::Reaction,
    V::Pin,
    V::Typing,
    V::Edit,
    V::Delete,
  ];
  let verb_names = [
    "text", "photo", "album", "document", "audio", "voice", "sticker", "reaction", "pin", "typing", "edit",
    "delete",
  ];
  assert_eq!(names(&verbs), verb_names);
  let capabilities: Vec<ChatCapability> = verbs.into_iter().map(ChatCapability::from).collect();
  assert_eq!(names(&capabilities), verb_names);
  use ChatCapability as C;
  assert_eq!(
    names(&[C::ReplyTo, C::FetchMedia, C::Avatars, C::History]),
    ["replyTo", "fetchMedia", "avatars", "history"]
  );
}

#[test]
fn message_with_every_field_null_that_may_be() {
  crosses(&message(), message_wire());
}

#[test]
fn message_in_an_album_answering_another() {
  let message = ChatMessage {
    reply_to_message_id: Some(4),
    album: Some(3),
    mode: ChatVoice::HumanReader,
    ..message()
  };
  let mut wire = message_wire();
  wire["replyToMessageId"] = json!(4);
  wire["album"] = json!(3);
  wire["mode"] = json!("human-reader");
  crosses(&message, wire);
}

#[test]
fn bare_handover() {
  let handover = ChatHandover {
    message: message(),
    context: None,
    room: None,
    files: vec![],
  };
  crosses(&handover, json!({ "message": message_wire() }));
}

#[test]
fn handover_with_context_room_and_files() {
  let handover = ChatHandover {
    message: message(),
    context: Some(InboundChatContext {
      room_title: Some("A Room".into()),
      sender_username: Some("person".into()),
      reply_to_sender_id: Some(43),
      spoken_text: Some("the caption".into()),
      metadata: Some(json!({ "anything": [1, 2] })),
      carries_file: true,
      gathering: true,
      tag: Some("tag-1".into()),
    }),
    room: Some(RoomFacts {
      kind: Some(RoomKind::Owner),
      identity: Some(99),
      private_to: Some(42),
    }),
    files: vec![MessageFile {
      id: "file-1".into(),
      name: "a.txt".into(),
      mime: "text/plain".into(),
      bytes: b"hi".to_vec(),
      kind: None,
    }],
  };
  crosses(
    &handover,
    json!({
      "message": message_wire(),
      "context": {
        "roomTitle": "A Room", "senderUsername": "person", "replyToSenderId": 43, "spokenText": "the caption",
        "metadata": { "anything": [1, 2] }, "carriesFile": true, "gathering": true, "tag": "tag-1",
      },
      "room": { "kind": "owner", "identity": 99, "privateTo": 42 },
      "files": [{ "id": "file-1", "name": "a.txt", "mime": "text/plain", "bytes": "aGk=" }],
    }),
  );
}

#[test]
fn a_message_file_without_an_id_is_not_read() {
  let file = json!({ "name": "a.txt", "mime": "text/plain", "bytes": "aGk=" });
  assert!(serde_json::from_value::<MessageFile>(file).is_err());
}

#[test]
fn empty_context_and_room_facts() {
  crosses(&InboundChatContext::default(), json!({}));
  crosses(&RoomFacts::default(), json!({}));
}

#[test]
fn gathered_kept() {
  let gathered = ChatGathered {
    chat_id: -100,
    message_id: 5,
    via: "platform".into(),
    outcome: GatheredOutcome::Kept {
      files: vec![MessageFile {
        id: "file-1".into(),
        name: "a.png".into(),
        mime: "image/png".into(),
        bytes: vec![137, 80],
        kind: Some(ChatFileKind::Photo),
      }],
    },
  };
  crosses(
    &gathered,
    json!({
      "chatId": -100, "messageId": 5, "via": "platform", "outcome": "kept",
      "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=", "kind": "photo" }],
    }),
  );
}

#[test]
fn gathered_not_kept() {
  let gathered = ChatGathered {
    chat_id: -100,
    message_id: 5,
    via: "platform".into(),
    outcome: GatheredOutcome::NotKept {
      reason: "The file was too large to keep.".into(),
    },
  };
  crosses(
    &gathered,
    json!({
      "chatId": -100, "messageId": 5, "via": "platform", "outcome": "not-kept",
      "reason": "The file was too large to keep.",
    }),
  );
}

#[test]
fn declaration() {
  let declaration = ChatDeclaration {
    key: "platform".into(),
    label: "Platform".into(),
    configured: None,
    modes: vec![
      ChatReadMode {
        voice: ChatVoice::BotAccount,
        reach: ChatReach::AllMessages,
        standing: ChatModeStanding::Active,
        reason: None,
        can_reply_to: true,
      },
      ChatReadMode {
        voice: ChatVoice::HumanReader,
        reach: ChatReach::MentionsOnly,
        standing: ChatModeStanding::Inactive,
        reason: Some("No account is configured.".into()),
        can_reply_to: false,
      },
    ],
    can: BTreeSet::from([ChatCapability::Text, ChatCapability::ReplyTo]),
    dialect: ChatDialectDeclaration {
      name: "plain".into(),
      limits: ChatDialectLimits {
        text: Some(4096),
        caption: None,
        album_photos: Some(10),
        file_bytes: None,
      },
      writing: "This room shows no formatting.".into(),
      foldable: false,
    },
    shows: Some("The whole line of the room.".into()),
    numbering: ChatNumbering {
      rooms: ChatNumberSpace::Native,
      people: ChatNumberSpace::Assigned,
      messages: ChatNumberSpace::None,
      ranges: ChatNumberRanges {
        rooms: None,
        people: Some(ChatNumberRange {
          first: 1_000_001,
          last: 2_000_000,
        }),
      },
    },
    speakers: true,
  };
  crosses(
    &declaration,
    json!({
      "key": "platform", "label": "Platform", "configured": null,
      "modes": [
        {
          "voice": "bot-account", "reach": "all-messages", "standing": "active", "reason": null,
          "canReplyTo": true,
        },
        {
          "voice": "human-reader", "reach": "mentions-only", "standing": "inactive",
          "reason": "No account is configured.", "canReplyTo": false,
        },
      ],
      "can": ["text", "replyTo"],
      "dialect": {
        "name": "plain",
        "limits": { "text": 4096, "caption": null, "albumPhotos": 10, "fileBytes": null },
        "writing": "This room shows no formatting.",
        "foldable": false,
      },
      "shows": "The whole line of the room.",
      "numbering": {
        "rooms": "native", "people": "assigned", "messages": "none",
        "ranges": { "rooms": null, "people": { "first": 1_000_001, "last": 2_000_000 } },
      },
      "speakers": true,
    }),
  );
}

#[test]
fn media_request() {
  crosses(
    &ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at: None,
    },
    json!({ "chatId": -100, "messageId": 5 }),
  );
  crosses(
    &ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at: Some(1_760_000_000_000),
    },
    json!({ "chatId": -100, "messageId": 5, "sentAt": 1_760_000_000_000_i64 }),
  );
}

#[test]
fn markup() {
  let markup = ChatMarkup {
    markup: "<b>hi</b>".into(),
    plain: "hi".into(),
    formatted: true,
  };
  crosses(&markup, json!({ "markup": "<b>hi</b>", "plain": "hi", "formatted": true }));
}

#[test]
fn error_shape_and_statuses() {
  let codes = [
    (ErrorCode::BadRequest, "bad-request", 400),
    (ErrorCode::Unauthorized, "unauthorized", 401),
    (ErrorCode::NoRoute, "no-route", 404),
    (ErrorCode::NothingHeld, "nothing-held", 404),
    (ErrorCode::NotReady, "not-ready", 409),
    (ErrorCode::Failed, "failed", 500),
    (ErrorCode::Unavailable, "unavailable", 404),
    (ErrorCode::UnsupportedCapability, "unsupported-capability", 422),
    (ErrorCode::NoPictureSent, "no-picture-sent", 422),
    (ErrorCode::SpeakerNotAllowed, "speaker-not-allowed", 403),
    (ErrorCode::SpeakerUnnumbered, "speaker-unnumbered", 422),
    (ErrorCode::Platform, "platform", 422),
    (ErrorCode::Gone, "gone", 410),
  ];
  assert_eq!(codes.len(), <ErrorCode as strum::VariantArray>::VARIANTS.len(), "every code is listed");
  for (code, name, status) in codes {
    assert_eq!(code.status(), status);
    crosses(
      &WireError::new(code, "What went wrong."),
      json!({ "code": name, "error": "What went wrong." }),
    );
  }
}

#[test]
fn the_list_of_kept_profile_pictures() {
  let list = AvatarList {
    avatars: vec![
      ChatAvatar {
        kind: ChatAvatarKind::Room,
        id: -100,
        taken_at: 1_760_000_000_000,
      },
      ChatAvatar {
        kind: ChatAvatarKind::Person,
        id: 42,
        taken_at: 1_760_000_000_500,
      },
    ],
  };
  crosses(
    &list,
    json!({ "avatars": [
      { "kind": "room", "id": -100, "takenAt": 1_760_000_000_000_i64 },
      { "kind": "person", "id": 42, "takenAt": 1_760_000_000_500_i64 },
    ] }),
  );
}

#[test]
fn the_receiver_questions_and_their_answers() {
  crosses(
    &ListenedQuery {
      chat_id: -100,
      via: "platform".into(),
    },
    json!({ "chatId": -100, "via": "platform" }),
  );
  crosses(&Listened { listened: false }, json!({ "listened": false }));
  crosses(
    &RoomsOfQuery {
      via: "platform".into(),
      after: None,
      limit: None,
    },
    json!({ "via": "platform" }),
  );
  crosses(
    &RoomsOfQuery {
      via: "platform".into(),
      after: Some(-100),
      limit: Some(50),
    },
    json!({ "via": "platform", "after": -100, "limit": 50 }),
  );
  crosses(
    &RoomsOf {
      rooms: vec![
        HistoryRoom {
          chat_id: -100,
          oldest_message_id: Some(5),
          oldest_sent_at: Some(1_760_000_000_000),
        },
        HistoryRoom {
          chat_id: 7,
          oldest_message_id: None,
          oldest_sent_at: None,
        },
      ],
    },
    json!({ "rooms": [
      { "chatId": -100, "oldestMessageId": 5, "oldestSentAt": 1_760_000_000_000_i64 },
      { "chatId": 7, "oldestMessageId": null, "oldestSentAt": null },
    ] }),
  );
  crosses(&HistoryKept { kept: true }, json!({ "kept": true }));
  crosses(
    &AvatarsUpdated {
      via: "platform".into(),
      rooms: vec![-100],
      people: vec![],
    },
    json!({ "via": "platform", "rooms": [-100], "people": [] }),
  );
}

#[test]
fn assigned_numbers_carry_their_ranges() {
  let numbering = ChatNumbering {
    rooms: ChatNumberSpace::Assigned,
    people: ChatNumberSpace::Assigned,
    messages: ChatNumberSpace::Assigned,
    ranges: ChatNumberRanges {
      rooms: Some(ChatNumberRange {
        first: i64::MIN,
        last: -9_000_000_000_000,
      }),
      people: Some(ChatNumberRange {
        first: 9_000_000_000_000,
        last: i64::MAX,
      }),
    },
  };
  crosses(
    &numbering,
    json!({
      "rooms": "assigned", "people": "assigned", "messages": "assigned",
      "ranges": {
        "rooms": { "first": i64::MIN, "last": -9_000_000_000_000_i64 },
        "people": { "first": 9_000_000_000_000_i64, "last": i64::MAX },
      },
    }),
  );
}
