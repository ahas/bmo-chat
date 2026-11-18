//! The pieces beside the sends cross the wire in the shape `docs/contract.md` and `docs/protocol.md` state,
//! and come back as they went.

use std::collections::BTreeSet;

use bmo_chat::{
  AvatarChange, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatCapability, ChatDeclaration,
  ChatDialectDeclaration, ChatDialectLimits, ChatFileKind, ChatHealth, ChatMarkup, ChatModeStanding,
  ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering, ChatReach, ChatReadMode, ChatVerb,
  ChatVoice, ErrorCode, FilesQuery, GatheredPatch, HandedMessage, InboundChatContext, LinePerson,
  MessageFile, MessageLine, MessageOrigin, Page, ProviderOwner, ProviderRoom, ReceiverRoom, RoomFacts,
  RoomKind, RoomQuery, RoomsQuery, WireError,
};
use serde_json::{Value, json};

fn crosses<T>(value: &T, wire: Value)
where
  T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
  assert_eq!(serde_json::to_value(value).expect("the value encodes"), wire);
  assert_eq!(&serde_json::from_value::<T>(wire).expect("the wire body decodes"), value);
}

fn message() -> HandedMessage {
  HandedMessage {
    origin: MessageOrigin::Live,
    sender_id: Some(42),
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000_000,
    text: "hello".into(),
    line: None,
    reply_to_message_id: None,
    via: "platform".into(),
    album: None,
    mode: ChatVoice::BotAccount,
    context: InboundChatContext::default(),
    room: RoomFacts::default(),
    files: vec![],
    gathering: false,
    gather_failure: None,
  }
}

fn message_wire() -> Value {
  json!({
    "origin": "live", "senderId": 42, "senderName": "A Person", "sentAt": 1_760_000_000_000_i64,
    "text": "hello", "replyToMessageId": null, "via": "platform", "album": null, "mode": "bot-account",
    "context": {}, "room": {}, "files": [], "gathering": false,
  })
}

fn file(kind: Option<ChatFileKind>) -> MessageFile {
  MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80].into(),
    kind,
  }
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
    names(&[BotAccount, HumanReader, HumanAvatar, OwnAccount, Direct]),
    ["bot-account", "human-reader", "human-avatar", "own-account", "direct"]
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
  assert_eq!(
    names(&[MessageOrigin::Live, MessageOrigin::Edit, MessageOrigin::History]),
    ["live", "edit", "history"]
  );
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
fn a_bare_message_with_every_field_null_that_may_be() {
  crosses(&message(), message_wire());
}

#[test]
fn a_message_read_back_in_an_album_answering_another() {
  let message = HandedMessage {
    origin: MessageOrigin::History,
    reply_to_message_id: Some(4),
    album: Some(3),
    mode: ChatVoice::HumanReader,
    ..message()
  };
  let mut wire = message_wire();
  wire["origin"] = json!("history");
  wire["replyToMessageId"] = json!(4);
  wire["album"] = json!(3);
  wire["mode"] = json!("human-reader");
  crosses(&message, wire);
}

#[test]
fn a_message_with_context_room_and_files() {
  let message = HandedMessage {
    origin: MessageOrigin::Edit,
    context: InboundChatContext {
      room_title: Some("A Room".into()),
      sender_username: Some("person".into()),
      reply_to_sender_id: Some(43),
      spoken_text: Some("the caption".into()),
      metadata: Some(json!({ "anything": [1, 2] })),
      carries_file: true,
      tag: Some("tag-1".into()),
    },
    room: RoomFacts {
      kind: Some(RoomKind::Owner),
      identity: Some(99),
      private_to: Some(42),
    },
    files: vec![file(Some(ChatFileKind::Photo))],
    ..message()
  };
  let mut wire = message_wire();
  wire["origin"] = json!("edit");
  wire["context"] = json!({
    "roomTitle": "A Room", "senderUsername": "person", "replyToSenderId": 43, "spokenText": "the caption",
    "metadata": { "anything": [1, 2] }, "carriesFile": true, "tag": "tag-1",
  });
  wire["room"] = json!({ "kind": "owner", "identity": 99, "privateTo": 42 });
  wire["files"] =
    json!([{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=", "kind": "photo" }]);
  crosses(&message, wire);
}

#[test]
fn a_joining_line_names_its_people_and_who_brought_them_where_that_is_someone_else() {
  let newcomers = [LinePerson::new(43, "A newcomer"), LinePerson::new(44, "Another newcomer")];
  let message = HandedMessage {
    sender_id: Some(43),
    text: "[Joined] A newcomer, Another newcomer".into(),
    line: Some(MessageLine::people_joined(&newcomers, None)),
    ..message()
  };
  let mut wire = message_wire();
  wire["senderId"] = json!(43);
  wire["text"] = json!("[Joined] A newcomer, Another newcomer");
  wire["line"] = json!({
    "code": "people-joined",
    "fields": { "people": [{ "senderId": 43, "name": "A newcomer" }, { "senderId": 44, "name": "Another newcomer" }] },
  });
  crosses(&message, wire);

  let brought = MessageLine::people_joined(&newcomers[..1], Some(&LinePerson::new(42, "A Person")));
  assert_eq!(
    serde_json::to_value(&brought).expect("the line encodes"),
    json!({
      "code": "people-joined",
      "fields": { "people": [{ "senderId": 43, "name": "A newcomer" }], "by": { "senderId": 42, "name": "A Person" } },
    })
  );
}

#[test]
fn a_leaving_line_names_its_person_and_who_took_them_out_where_that_is_someone_else() {
  let leaver = LinePerson::new(43, "A newcomer");
  assert_eq!(
    serde_json::to_value(MessageLine::person_left(&leaver, None)).expect("the line encodes"),
    json!({ "code": "person-left", "fields": { "person": { "senderId": 43, "name": "A newcomer" } } })
  );
  assert_eq!(
    serde_json::to_value(MessageLine::person_left(&leaver, Some(&LinePerson::new(42, "A Person"))))
      .expect("the line encodes"),
    json!({
      "code": "person-left",
      "fields": { "person": { "senderId": 43, "name": "A newcomer" }, "by": { "senderId": 42, "name": "A Person" } },
    })
  );
}

#[test]
fn a_message_gathering_its_files_and_one_whose_gathering_failed() {
  let gathering = HandedMessage {
    gathering: true,
    ..message()
  };
  let mut wire = message_wire();
  wire["gathering"] = json!(true);
  crosses(&gathering, wire);
  let failed = HandedMessage {
    gather_failure: Some("The file was too large to keep.".into()),
    ..message()
  };
  let mut wire = message_wire();
  wire["gatherFailure"] = json!("The file was too large to keep.");
  crosses(&failed, wire);
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
fn gathered_files_kept_and_not_kept() {
  crosses(
    &GatheredPatch::kept(vec![file(None)]),
    json!({ "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }] }),
  );
  crosses(
    &GatheredPatch::not_kept("The file was too large to keep."),
    json!({ "files": [], "gatherFailure": "The file was too large to keep." }),
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
fn health_room_and_owner() {
  crosses(
    &ChatHealth::ready("1.2.3"),
    json!({ "ready": true, "reason": null, "version": "1.2.3" }),
  );
  crosses(
    &ChatHealth::not_ready("1.2.3", "No token is configured."),
    json!({ "ready": false, "reason": "No token is configured.", "version": "1.2.3" }),
  );
  crosses(
    &ProviderRoom {
      id: -100,
      owns: true,
      identity: Some(7),
      kind: RoomKind::Group,
      private_to: None,
      title: Some("A Room".into()),
      modes: None,
    },
    json!({
      "id": -100, "owns": true, "identity": 7, "kind": "group", "privateTo": null, "title": "A Room",
      "modes": null,
    }),
  );
  crosses(
    &ProviderRoom {
      id: -200,
      owns: true,
      identity: None,
      kind: RoomKind::Group,
      private_to: None,
      title: None,
      modes: Some(vec![ChatVoice::HumanReader]),
    },
    json!({
      "id": -200, "owns": true, "identity": null, "kind": "group", "privateTo": null, "title": null,
      "modes": ["human-reader"],
    }),
  );
  crosses(&ProviderOwner { room_id: Some(42) }, json!({ "roomId": 42 }));
  crosses(&ProviderOwner { room_id: None }, json!({ "roomId": null }));
}

#[test]
fn the_files_query() {
  crosses(&FilesQuery { sent_at: None }, json!({}));
  crosses(
    &FilesQuery {
      sent_at: Some(1_760_000_000_000),
    },
    json!({ "sentAt": 1_760_000_000_000_i64 }),
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
    (ErrorCode::Forbidden, "forbidden", 403),
    (ErrorCode::NoRoute, "no-route", 404),
    (ErrorCode::NotFound, "not-found", 404),
    (ErrorCode::Conflict, "conflict", 409),
    (ErrorCode::Gone, "gone", 410),
    (ErrorCode::TooLarge, "too-large", 413),
    (ErrorCode::Failed, "failed", 500),
    (ErrorCode::UpstreamFailed, "upstream-failed", 502),
    (ErrorCode::NotReady, "not-ready", 503),
    (ErrorCode::Unavailable, "unavailable", 404),
    (ErrorCode::UnsupportedCapability, "unsupported-capability", 422),
    (ErrorCode::NoPictureSent, "no-picture-sent", 422),
    (ErrorCode::SpeakerNotAllowed, "speaker-not-allowed", 403),
    (ErrorCode::SpeakerUnnumbered, "speaker-unnumbered", 422),
    (ErrorCode::Platform, "platform", 422),
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
fn a_page_of_kept_profile_pictures_and_its_query() {
  let page = Page {
    items: vec![
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
    more: true,
  };
  crosses(
    &page,
    json!({
      "items": [
        { "kind": "room", "id": -100, "takenAt": 1_760_000_000_000_i64 },
        { "kind": "person", "id": 42, "takenAt": 1_760_000_000_500_i64 },
      ],
      "more": true,
    }),
  );
  crosses(&AvatarsQuery::default(), json!({}));
  crosses(
    &AvatarsQuery {
      after: Some(1_000),
      limit: Some(2),
    },
    json!({ "after": 1_000, "limit": 2 }),
  );
}

#[test]
fn the_receiving_side_rooms_queries_and_word_of_pictures() {
  crosses(
    &RoomQuery {
      via: "platform".into(),
    },
    json!({ "via": "platform" }),
  );
  crosses(
    &RoomsQuery {
      via: "platform".into(),
      after: None,
      limit: None,
    },
    json!({ "via": "platform" }),
  );
  crosses(
    &RoomsQuery {
      via: "platform".into(),
      after: Some(-100),
      limit: Some(50),
    },
    json!({ "via": "platform", "after": -100, "limit": 50 }),
  );
  crosses(
    &Page {
      items: vec![
        ReceiverRoom {
          id: -100,
          listened: true,
          read: true,
          oldest_message_id: Some(5),
          oldest_sent_at: Some(1_760_000_000_000),
        },
        ReceiverRoom {
          id: 7,
          listened: false,
          read: false,
          oldest_message_id: None,
          oldest_sent_at: None,
        },
      ],
      more: false,
    },
    json!({
      "items": [
        { "id": -100, "listened": true, "read": true, "oldestMessageId": 5, "oldestSentAt": 1_760_000_000_000_i64 },
        { "id": 7, "listened": false, "read": false, "oldestMessageId": null, "oldestSentAt": null },
      ],
      "more": false,
    }),
  );
  crosses(
    &AvatarChange {
      via: "platform".into(),
      room_ids: vec![-100],
      person_ids: vec![],
    },
    json!({ "via": "platform", "roomIds": [-100], "personIds": [] }),
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

#[test]
fn a_room_from_a_side_that_does_not_say_takes_every_mode_and_is_not_asked_to_be_read() {
  let room: ProviderRoom = serde_json::from_value(json!({
    "id": -100, "owns": true, "identity": 7, "kind": "group", "privateTo": null, "title": null,
  }))
  .expect("a room without modes decodes");
  assert_eq!(room.modes, None);
  let held: ReceiverRoom = serde_json::from_value(json!({
    "id": -100, "listened": true, "oldestMessageId": null, "oldestSentAt": null,
  }))
  .expect("a held room without read decodes");
  assert!(!held.read);
}
