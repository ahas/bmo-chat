//! The two base libraries agree on the wire. Every piece both crates define crosses from one crate's
//! encoder into the other's decoder, both ways. Over the loopback, the real provider router serves a
//! stand-in provider to the real provider client, and the real receiver link posts into the real receiver
//! router. No test waits on the real clock, and every listener a test starts is taken down before it ends.

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-shared-token";

/// The fixtures, written once and built in each crate's own types. A struct literal names every field its
/// type has, so a field one crate has and the other lacks fails to compile here.
macro_rules! fixtures {
  ($side:ident, $krate:ident) => {
    mod $side {
      use $krate::*;
      use serde_json::json;

      fn file(name: &str, mime: &str, bytes: &[u8]) -> FileItem {
        FileItem {
          name: name.into(),
          mime: mime.into(),
          bytes: bytes.to_vec(),
        }
      }

      fn message_file(id: &str, name: &str, mime: &str, bytes: &[u8]) -> MessageFile {
        MessageFile {
          id: id.into(),
          name: name.into(),
          mime: mime.into(),
          bytes: bytes.to_vec(),
          kind: None,
        }
      }

      fn message(chat_id: i64, message_id: i64) -> ChatMessage {
        ChatMessage {
          chat_id,
          message_id,
          sender_id: None,
          sender_name: "A person".into(),
          sent_at: 1_760_000_000_000,
          text: "Hello".into(),
          reply_to_message_id: None,
          via: "stand-in".into(),
          album: None,
          mode: ChatVoice::BotAccount,
        }
      }

      fn label() -> String {
        "Stand-in".into()
      }

      /// Every send verb, each with every field it takes set and, where it has any, with none of its
      /// optional fields.
      pub fn sends() -> Vec<ChatSendRequest> {
        vec![
          ChatSendRequest {
            chat_id: -100,
            send: ChatSend::Text {
              text: "Hello **there**".into(),
              reply_to: Some(41),
              silent: true,
              fold: true,
            },
            voice: Some(ChatVoice::HumanAvatar),
            speaker: Some(ChatSpeaker {
              agent: "helper".into(),
            }),
            tag: Some("send-1".into()),
            prefix: Some("[agent] ".into()),
          },
          ChatSendRequest::new(
            7,
            ChatSend::Text {
              text: "Just words".into(),
              reply_to: None,
              silent: false,
              fold: false,
            },
          ),
          ChatSendRequest {
            chat_id: -100,
            send: ChatSend::Photo {
              image_url: "https://example.com/picture.jpg".into(),
              caption: Some("A picture".into()),
              reply_to: Some(42),
            },
            voice: Some(ChatVoice::BotAccount),
            speaker: None,
            tag: None,
            prefix: None,
          },
          ChatSendRequest::new(
            7,
            ChatSend::Photo {
              image_url: "/data/picture.png".into(),
              caption: None,
              reply_to: None,
            },
          ),
          ChatSendRequest {
            chat_id: -100,
            send: ChatSend::Album {
              items: vec![
                AlbumItem::Address {
                  address: "https://example.com/one.jpg".into(),
                },
                AlbumItem::File(file("two.png", "image/png", &[0x89, b'P', b'N', b'G', 0, 255])),
              ],
              caption: Some("Two pictures".into()),
              reply_to: Some(43),
            },
            voice: Some(ChatVoice::HumanReader),
            speaker: None,
            tag: Some("album-1".into()),
            prefix: None,
          },
          ChatSendRequest::new(
            7,
            ChatSend::Album {
              items: vec![AlbumItem::Address {
                address: "/data/one.jpg".into(),
              }],
              caption: None,
              reply_to: None,
            },
          ),
          ChatSendRequest::new(
            7,
            ChatSend::Document {
              bytes: b"%PDF-1.7 a document".to_vec(),
              file_name: "notes.pdf".into(),
              reply_to: Some(44),
            },
          ),
          ChatSendRequest::new(
            7,
            ChatSend::Audio {
              audio: vec![0xff, 0xfb, 0x90, 0x00],
              file_name: "song.mp3".into(),
              reply_to: None,
            },
          ),
          ChatSendRequest {
            chat_id: 7,
            send: ChatSend::Voice {
              audio: vec![b'O', b'g', b'g', b'S', 0],
              said: "Good morning".into(),
              caption: Some("A greeting".into()),
              transcript: Some("Good morning!".into()),
              reply_to: Some(45),
            },
            voice: Some(ChatVoice::HumanAsAgent),
            speaker: None,
            tag: None,
            prefix: None,
          },
          ChatSendRequest::new(
            7,
            ChatSend::Voice {
              audio: vec![1, 2, 3],
              said: "Bye".into(),
              caption: None,
              transcript: None,
              reply_to: None,
            },
          ),
          ChatSendRequest::new(
            7,
            ChatSend::Sticker {
              emoji: "\u{1f44d}".into(),
            },
          ),
          ChatSendRequest::new(
            7,
            ChatSend::Reaction {
              message_id: 46,
              emoji: "\u{2764}".into(),
            },
          ),
          ChatSendRequest::new(
            7,
            ChatSend::Reaction {
              message_id: 46,
              emoji: String::new(),
            },
          ),
          ChatSendRequest::new(
            -100,
            ChatSend::Pin {
              message_id: 47,
              on: true,
              silent: true,
            },
          ),
          ChatSendRequest::new(
            -100,
            ChatSend::Pin {
              message_id: 47,
              on: false,
              silent: false,
            },
          ),
          ChatSendRequest {
            chat_id: 0,
            send: ChatSend::Typing { on: true },
            voice: Some(ChatVoice::Direct),
            speaker: None,
            tag: None,
            prefix: None,
          },
          ChatSendRequest::new(
            7,
            ChatSend::Edit {
              message_id: 48,
              text: "Said again".into(),
            },
          ),
          ChatSendRequest::new(7, ChatSend::Delete { message_id: 49 }),
        ]
      }

      fn missed() -> Vec<MissedPicture> {
        [
          MissedPictureReason::NotAnAddress,
          MissedPictureReason::OutsideDataRoot,
          MissedPictureReason::MissingFile,
          MissedPictureReason::Unfetched,
          MissedPictureReason::Platform {
            detail: "The platform took no picture that large.".into(),
          },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, reason)| MissedPicture {
          address: format!("/data/picture-{index}.jpg"),
          reason,
        })
        .collect()
      }

      /// Every way a performed send comes out: with and without a number, and with missed pictures.
      pub fn sents() -> Vec<ChatSent> {
        vec![
          ChatSent {
            message_id: Some(501),
            missed: vec![],
          },
          ChatSent {
            message_id: None,
            missed: vec![],
          },
          ChatSent {
            message_id: Some(502),
            missed: missed(),
          },
        ]
      }

      pub fn media_requests() -> Vec<ChatMediaRequest> {
        vec![
          ChatMediaRequest {
            chat_id: -100,
            message_id: 60,
            sent_at: Some(1_760_000_100_000),
          },
          ChatMediaRequest {
            chat_id: 7,
            message_id: 61,
            sent_at: None,
          },
        ]
      }

      pub fn fetched() -> MessageFile {
        message_file("photo-5", "picture.jpg", "image/jpeg", &[0xff, 0xd8, 0xff, 0xe0])
      }

      /// A provider that can do everything, in every mode, and one that is not configured and declares
      /// next to nothing. The stand-in provider over the loopback is the first.
      pub fn declarations() -> Vec<ChatDeclaration> {
        vec![
          ChatDeclaration {
            key: "stand-in".into(),
            label: label(),
            configured: None,
            modes: vec![
              ChatReadMode {
                voice: ChatVoice::BotAccount,
                reach: ChatReach::MentionsOnly,
                standing: ChatModeStanding::Active,
                reason: None,
                can_reply_to: true,
              },
              ChatReadMode {
                voice: ChatVoice::HumanReader,
                reach: ChatReach::AllMessages,
                standing: ChatModeStanding::Active,
                reason: None,
                can_reply_to: false,
              },
              ChatReadMode {
                voice: ChatVoice::HumanAvatar,
                reach: ChatReach::AllMessages,
                standing: ChatModeStanding::Inactive,
                reason: Some("No account is signed in.".into()),
                can_reply_to: true,
              },
              ChatReadMode {
                voice: ChatVoice::HumanAsAgent,
                reach: ChatReach::AllMessages,
                standing: ChatModeStanding::Unsupported,
                reason: Some("The platform's terms forbid it.".into()),
                can_reply_to: false,
              },
              ChatReadMode {
                voice: ChatVoice::Direct,
                reach: ChatReach::AllMessages,
                standing: ChatModeStanding::Active,
                reason: None,
                can_reply_to: true,
              },
            ],
            can: ChatCapability::ALL.iter().copied().collect(),
            dialect: ChatDialectDeclaration {
              name: "stand-in-markup".into(),
              limits: ChatDialectLimits {
                text: Some(4096),
                caption: Some(1024),
                album_photos: Some(10),
                file_bytes: Some(50_000_000),
              },
              writing: "Bold and links show; everything else is stripped.".into(),
              foldable: true,
            },
            shows: Some("Readers also see who is typing.".into()),
            numbering: ChatNumbering {
              rooms: ChatNumberSpace::Native,
              people: ChatNumberSpace::Assigned,
              messages: ChatNumberSpace::None,
              ranges: ChatNumberRanges {
                rooms: None,
                people: Some(ChatNumberRange { first: 1_000_001, last: 2_000_000 }),
              },
            },
            speakers: true,
          },
          ChatDeclaration {
            key: "unset".into(),
            label: "Unset".into(),
            configured: Some("No token is set.".into()),
            modes: vec![],
            can: [ChatCapability::Text].into_iter().collect(),
            dialect: ChatDialectDeclaration {
              name: "plain".into(),
              limits: ChatDialectLimits::default(),
              writing: "This room shows no formatting.".into(),
              foldable: false,
            },
            shows: None,
            numbering: ChatNumbering {
              rooms: ChatNumberSpace::Native,
              people: ChatNumberSpace::Native,
              messages: ChatNumberSpace::Native,
              ranges: ChatNumberRanges::default(),
            },
            speakers: false,
          },
        ]
      }

      pub fn healths() -> Vec<ChatHealth> {
        vec![
          ChatHealth {
            ready: true,
            reason: None,
          },
          ChatHealth::not_ready("The platform connection is not open yet."),
        ]
      }

      /// The rooms asked about, and below, each room route's answer for each of them in the same order.
      pub fn room_queries() -> Vec<RoomQuery> {
        vec![RoomQuery { chat_id: -100 }, RoomQuery { chat_id: 7 }]
      }

      pub fn room_owns() -> Vec<RoomOwns> {
        vec![RoomOwns { owns: true }, RoomOwns { owns: false }]
      }

      pub fn room_identities() -> Vec<RoomIdentity> {
        vec![
          RoomIdentity {
            identity: Some(7001),
          },
          RoomIdentity { identity: None },
        ]
      }

      pub fn room_kinds() -> Vec<RoomKindAnswer> {
        vec![
          RoomKindAnswer {
            kind: RoomKind::Group,
          },
          RoomKindAnswer {
            kind: RoomKind::Owner,
          },
        ]
      }

      pub fn room_persons() -> Vec<RoomPerson> {
        vec![RoomPerson { person: None }, RoomPerson { person: Some(42) }]
      }

      pub fn room_titles() -> Vec<RoomTitle> {
        vec![
          RoomTitle {
            title: Some("The garden".into()),
          },
          RoomTitle { title: None },
        ]
      }

      pub fn owner_rooms() -> Vec<OwnerRoom> {
        vec![OwnerRoom { chat_id: Some(7) }, OwnerRoom { chat_id: None }]
      }

      /// The texts read, and below, the reading of each in the same order.
      pub fn dialect_reads() -> Vec<DialectRead> {
        vec![
          DialectRead {
            text: "Some **bold** words".into(),
          },
          DialectRead {
            text: "A long answer\n\nin two parts".into(),
          },
        ]
      }

      pub fn dialect_readings() -> Vec<DialectReading> {
        vec![
          DialectReading {
            markup: ChatMarkup {
              markup: "Some <b>bold</b> words".into(),
              plain: "Some bold words".into(),
              formatted: true,
            },
            parts: vec!["Some **bold** words".into()],
            fold: None,
          },
          DialectReading {
            markup: ChatMarkup {
              markup: "A long answer\n\nin two parts".into(),
              plain: "A long answer\n\nin two parts".into(),
              formatted: false,
            },
            parts: vec!["A long answer".into(), "in two parts".into()],
            fold: Some("<details>A long answer\n\nin two parts</details>".into()),
          },
        ]
      }

      /// The markup checked, and below, the check of each in the same order.
      pub fn dialect_checks() -> Vec<DialectCheck> {
        vec![
          DialectCheck {
            markup: "Some <b>bold</b> words".into(),
          },
          DialectCheck {
            markup: "<b>open".into(),
          },
        ]
      }

      pub fn dialect_checkeds() -> Vec<DialectChecked> {
        vec![
          DialectChecked {
            fault: None,
            visible: "Some bold words".into(),
          },
          DialectChecked {
            fault: Some("A <b> is never closed.".into()),
            visible: "open".into(),
          },
        ]
      }

      /// The body of the `message`, `attachment` and `edited` hand-overs: bare, with every field set, and
      /// carrying its files.
      pub fn handovers() -> Vec<ChatHandover> {
        vec![
          ChatHandover {
            message: message(-100, 1),
            context: None,
            room: None,
            files: vec![],
          },
          ChatHandover {
            message: ChatMessage {
              chat_id: 7,
              message_id: 2,
              sender_id: Some(42),
              sender_name: "Another person".into(),
              sent_at: 1_760_000_200_000,
              text: "[picture]".into(),
              reply_to_message_id: Some(1),
              via: "stand-in".into(),
              album: Some(2),
              mode: ChatVoice::HumanReader,
            },
            context: Some(InboundChatContext {
              room_title: Some("The garden".into()),
              sender_username: Some("another".into()),
              reply_to_sender_id: Some(7001),
              spoken_text: Some("Look at this".into()),
              metadata: Some(json!({ "platform": { "kind": "photo", "sizes": [320, 1280] } })),
              carries_file: true,
              gathering: true,
              tag: Some("send-1".into()),
            }),
            room: Some(RoomFacts {
              kind: Some(RoomKind::Owner),
              identity: Some(7001),
              private_to: Some(42),
            }),
            files: vec![],
          },
          ChatHandover {
            message: message(-100, 3),
            context: Some(InboundChatContext {
              carries_file: true,
              ..InboundChatContext::default()
            }),
            room: Some(RoomFacts {
              kind: Some(RoomKind::Group),
              identity: None,
              private_to: None,
            }),
            files: vec![
              message_file("0", "notes.txt", "text/plain", b"some notes"),
              message_file("1", "tiny.png", "image/png", &[0x89, b'P', b'N', b'G']),
            ],
          },
        ]
      }

      pub fn gathered() -> Vec<ChatGathered> {
        vec![
          ChatGathered {
            chat_id: 7,
            message_id: 2,
            via: "stand-in".into(),
            outcome: GatheredOutcome::Kept {
              files: vec![message_file("photo-2", "picture.jpg", "image/jpeg", &[0xff, 0xd8, 0xff])],
            },
          },
          ChatGathered {
            chat_id: 7,
            message_id: 3,
            via: "stand-in".into(),
            outcome: GatheredOutcome::Kept { files: kinded_files() },
          },
          ChatGathered {
            chat_id: 7,
            message_id: 2,
            via: "stand-in".into(),
            outcome: GatheredOutcome::NotKept {
              reason: "The file is larger than the platform hands out.".into(),
            },
          },
        ]
      }

      /// Every kind of file, each on one message file.
      pub fn kinded_files() -> Vec<MessageFile> {
        use ChatFileKind::*;
        [Photo, Video, RoundVideo, Animation, Audio, VoiceNote, Sticker, Document]
          .into_iter()
          .enumerate()
          .map(|(at, kind)| MessageFile {
            kind: Some(kind),
            ..message_file(&at.to_string(), "file.bin", "application/octet-stream", &[at as u8])
          })
          .collect()
      }

      /// The pictures the stand-in provider keeps, in the order the list answers them.
      pub fn avatars() -> Vec<ChatAvatar> {
        vec![
          ChatAvatar {
            kind: ChatAvatarKind::Room,
            id: -100,
            taken_at: 1_760_000_000_000,
          },
          ChatAvatar {
            kind: ChatAvatarKind::Person,
            id: 42,
            taken_at: 1_760_000_100_000,
          },
        ]
      }

      pub fn avatar_lists() -> Vec<AvatarList> {
        vec![AvatarList { avatars: avatars() }, AvatarList { avatars: vec![] }]
      }

      pub fn avatar_queries() -> Vec<AvatarsQuery> {
        vec![
          AvatarsQuery::default(),
          AvatarsQuery {
            after: Some(1_760_000_000_000),
            limit: Some(1),
          },
        ]
      }

      pub fn avatars_updated() -> Vec<AvatarsUpdated> {
        vec![AvatarsUpdated {
          via: "stand-in".into(),
          rooms: vec![-100],
          people: vec![42, 43],
        }]
      }

      /// The rooms asked about, and below, whether each is listened to, in the same order.
      pub fn listened_queries() -> Vec<ListenedQuery> {
        vec![
          ListenedQuery {
            chat_id: -100,
            via: "stand-in".into(),
          },
          ListenedQuery {
            chat_id: 7,
            via: "stand-in".into(),
          },
        ]
      }

      pub fn listened() -> Vec<Listened> {
        vec![Listened { listened: true }, Listened { listened: false }]
      }

      /// The queries for rooms to read back, and below, the answer to each, in the same order.
      pub fn rooms_of_queries() -> Vec<RoomsOfQuery> {
        vec![
          RoomsOfQuery {
            via: "stand-in".into(),
            after: None,
            limit: None,
          },
          RoomsOfQuery {
            via: "stand-in".into(),
            after: Some(-100),
            limit: Some(1),
          },
        ]
      }

      pub fn rooms_of() -> Vec<RoomsOf> {
        vec![
          RoomsOf {
            rooms: vec![
              HistoryRoom {
                chat_id: -100,
                oldest_message_id: Some(1),
                oldest_sent_at: Some(1_760_000_000_000),
              },
              HistoryRoom {
                chat_id: 7,
                oldest_message_id: None,
                oldest_sent_at: None,
              },
            ],
          },
          RoomsOf { rooms: vec![] },
        ]
      }

      /// The answers to the history hand-overs, in the order of the hand-over fixtures.
      pub fn history_kept() -> Vec<HistoryKept> {
        vec![HistoryKept { kept: true }, HistoryKept { kept: false }, HistoryKept { kept: true }]
      }

      /// The error shape with every code, the refusals' among them.
      pub fn wire_errors() -> Vec<WireError> {
        vec![
          WireError::new(ErrorCode::BadRequest, "The body is not the shape this route takes."),
          WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong."),
          WireError::new(ErrorCode::NoRoute, "There is no route at GET /nowhere."),
          WireError::new(ErrorCode::NothingHeld, "No profile picture is kept for room 7."),
          WireError::new(ErrorCode::NotReady, "The receiver is still starting."),
          WireError::new(ErrorCode::Failed, "The archive could not be written."),
          WireError::new(ErrorCode::Unavailable, "The platform no longer keeps that file."),
          WireError::new(ErrorCode::UnsupportedCapability, "Stand-in cannot send a sticker."),
          WireError::new(ErrorCode::NoPictureSent, "Stand-in: no picture was sent."),
          WireError::new(ErrorCode::SpeakerNotAllowed, "Stand-in cannot send in the name of agent helper."),
          WireError::new(
            ErrorCode::SpeakerUnnumbered,
            "Stand-in has no number for agent helper, so it cannot send in that agent's name.",
          ),
          WireError::new(ErrorCode::Platform, "Stand-in: That message cannot be pinned."),
          WireError::new(ErrorCode::Gone, "Stand-in: That message can no longer be edited."),
        ]
      }
    }
  };
}

fixtures!(on_provider, bmo_chat);
fixtures!(on_client, bmo_chat_client);

/// Each fixture of a family, built by each crate, is written by both encoders as the same body, and each
/// crate's body is read by the other crate as the other crate's own fixture.
fn crosses<P, C>(family: &str, provider: Vec<P>, client: Vec<C>)
where
  P: Serialize + DeserializeOwned + PartialEq + Debug,
  C: Serialize + DeserializeOwned + PartialEq + Debug,
{
  assert_eq!(provider.len(), client.len(), "{family}: both crates build as many fixtures");
  assert!(!provider.is_empty(), "{family}: there is something to cross");
  for (index, (built_by_provider, built_by_client)) in provider.iter().zip(&client).enumerate() {
    let provider_wire = serde_json::to_vec(built_by_provider).expect("a fixture encodes");
    let client_wire = serde_json::to_vec(built_by_client).expect("a fixture encodes");
    let as_json = |wire: &[u8]| serde_json::from_slice::<Value>(wire).expect("an encoder writes JSON");
    assert_eq!(
      as_json(&provider_wire),
      as_json(&client_wire),
      "{family} {index}: the two encoders write different bodies"
    );
    let read_by_client: C = serde_json::from_slice(&provider_wire).unwrap_or_else(|error| {
      panic!("{family} {index}: the client cannot read the provider's body: {error}")
    });
    assert_eq!(&read_by_client, built_by_client, "{family} {index}: provider to client");
    let read_by_provider: P = serde_json::from_slice(&client_wire).unwrap_or_else(|error| {
      panic!("{family} {index}: the provider cannot read the client's body: {error}")
    });
    assert_eq!(&read_by_provider, built_by_provider, "{family} {index}: client to provider");
  }
}

#[test]
fn every_send_and_what_it_came_to_crosses_both_ways() {
  crosses("send", on_provider::sends(), on_client::sends());
  crosses("sent", on_provider::sents(), on_client::sents());
  crosses("media request", on_provider::media_requests(), on_client::media_requests());
}

#[test]
fn both_crates_answer_every_code_with_the_same_status() {
  use strum::VariantArray;
  let provider = bmo_chat::ErrorCode::VARIANTS;
  let client = bmo_chat_client::ErrorCode::VARIANTS;
  assert_eq!(provider.len(), client.len(), "both crates have every code");
  for code in provider {
    let read: bmo_chat_client::ErrorCode =
      serde_json::from_value(serde_json::to_value(code).unwrap()).unwrap();
    assert_eq!(code.status(), read.status(), "{code:?}");
  }
}

#[test]
fn every_declaration_and_health_crosses_both_ways() {
  crosses("declaration", on_provider::declarations(), on_client::declarations());
  crosses("health", on_provider::healths(), on_client::healths());
  crosses("avatar list", on_provider::avatar_lists(), on_client::avatar_lists());
  crosses("avatar query", on_provider::avatar_queries(), on_client::avatar_queries());
}

#[test]
fn every_room_and_dialect_query_and_its_answer_crosses_both_ways() {
  crosses("room query", on_provider::room_queries(), on_client::room_queries());
  crosses("room/owns", on_provider::room_owns(), on_client::room_owns());
  crosses("room/identity", on_provider::room_identities(), on_client::room_identities());
  crosses("room/is-owner", on_provider::room_kinds(), on_client::room_kinds());
  crosses("room/private-of", on_provider::room_persons(), on_client::room_persons());
  crosses("room/title", on_provider::room_titles(), on_client::room_titles());
  crosses("owner-room", on_provider::owner_rooms(), on_client::owner_rooms());
  crosses("dialect/read", on_provider::dialect_reads(), on_client::dialect_reads());
  crosses("dialect reading", on_provider::dialect_readings(), on_client::dialect_readings());
  crosses("dialect/check", on_provider::dialect_checks(), on_client::dialect_checks());
  crosses("dialect check", on_provider::dialect_checkeds(), on_client::dialect_checkeds());
}

#[test]
fn every_inbound_body_and_the_error_shape_cross_both_ways() {
  crosses("hand-over", on_provider::handovers(), on_client::handovers());
  crosses("gathered", on_provider::gathered(), on_client::gathered());
  crosses("history kept", on_provider::history_kept(), on_client::history_kept());
  crosses("avatars/updated", on_provider::avatars_updated(), on_client::avatars_updated());
  crosses("rooms/listened", on_provider::listened_queries(), on_client::listened_queries());
  crosses("listened", on_provider::listened(), on_client::listened());
  crosses("rooms/of", on_provider::rooms_of_queries(), on_client::rooms_of_queries());
  crosses("rooms of", on_provider::rooms_of(), on_client::rooms_of());
  crosses("error", on_provider::wire_errors(), on_client::wire_errors());
}

#[test]
fn the_fixtures_hold_every_member_of_every_set() {
  use bmo_chat::{
    ChatAvatarKind, ChatCapability, ChatFileKind, ChatModeStanding, ChatNumberSpace, ChatReach, ChatVerb,
    ChatVoice, ErrorCode, GatheredOutcome, RoomKind,
  };

  let verbs: BTreeSet<ChatVerb> = on_provider::sends().iter().map(|request| request.send.verb()).collect();
  assert_eq!(verbs, ChatVerb::ALL.iter().copied().collect());

  let missed: Vec<_> = on_provider::sents().into_iter().flat_map(|sent| sent.missed).collect();
  let reasons: BTreeSet<_> = missed.iter().map(|picture| picture.reason.code()).collect();
  assert_eq!(
    reasons,
    BTreeSet::from(["not-an-address", "outside-data-root", "missing-file", "unfetched", "platform"])
  );
  let refused: Vec<ErrorCode> = refusals().iter().map(bmo_chat::ChatRefusal::code).collect();
  for code in [
    ErrorCode::UnsupportedCapability,
    ErrorCode::NoPictureSent,
    ErrorCode::SpeakerNotAllowed,
    ErrorCode::SpeakerUnnumbered,
    ErrorCode::NotReady,
    ErrorCode::Platform,
    ErrorCode::Gone,
  ] {
    assert!(refused.contains(&code), "the stand-in refuses with {code:?}");
  }

  let declarations = on_provider::declarations();
  let modes: Vec<_> = declarations.iter().flat_map(|declaration| declaration.modes.clone()).collect();
  for voice in [
    ChatVoice::BotAccount,
    ChatVoice::HumanReader,
    ChatVoice::HumanAvatar,
    ChatVoice::HumanAsAgent,
    ChatVoice::Direct,
  ] {
    assert!(modes.iter().any(|mode| mode.voice == voice), "a mode speaks with {voice:?}");
    assert!(
      on_provider::sends().iter().any(|request| request.voice == Some(voice)),
      "a send speaks as {voice:?}"
    );
  }
  for reach in [ChatReach::AllMessages, ChatReach::MentionsOnly] {
    assert!(modes.iter().any(|mode| mode.reach == reach), "a mode reaches {reach:?}");
  }
  for standing in [ChatModeStanding::Active, ChatModeStanding::Inactive, ChatModeStanding::Unsupported] {
    assert!(modes.iter().any(|mode| mode.standing == standing), "a mode stands {standing:?}");
  }
  for space in [ChatNumberSpace::Native, ChatNumberSpace::Assigned, ChatNumberSpace::None] {
    assert!(
      declarations.iter().any(|declaration| {
        let numbering = declaration.numbering;
        [numbering.rooms, numbering.people, numbering.messages].contains(&space)
      }),
      "a declaration numbers with {space:?}"
    );
  }
  assert_eq!(declarations[0].can, ChatCapability::ALL.iter().copied().collect());

  for kind in [RoomKind::Owner, RoomKind::Group] {
    assert!(on_provider::room_kinds().iter().any(|answer| answer.kind == kind));
    assert!(
      on_provider::handovers()
        .iter()
        .any(|handover| handover.room.and_then(|room| room.kind) == Some(kind))
    );
  }
  let kinds: Vec<Option<ChatFileKind>> = on_provider::kinded_files().iter().map(|file| file.kind).collect();
  let every_kind = [
    ChatFileKind::Photo,
    ChatFileKind::Video,
    ChatFileKind::RoundVideo,
    ChatFileKind::Animation,
    ChatFileKind::Audio,
    ChatFileKind::VoiceNote,
    ChatFileKind::Sticker,
    ChatFileKind::Document,
  ];
  assert_eq!(kinds, every_kind.map(Some));
  for kind in [ChatAvatarKind::Room, ChatAvatarKind::Person] {
    assert!(on_provider::avatars().iter().any(|avatar| avatar.kind == kind));
  }
  assert!(on_provider::sends().iter().any(|request| request.prefix.is_some()));
  let gathered = on_provider::gathered();
  assert!(gathered.iter().any(|body| matches!(body.outcome, GatheredOutcome::Kept { .. })));
  assert!(gathered.iter().any(|body| matches!(body.outcome, GatheredOutcome::NotKept { .. })));
  let codes: Vec<ErrorCode> = on_provider::wire_errors().iter().map(|error| error.code).collect();
  assert_eq!(codes, <ErrorCode as strum::VariantArray>::VARIANTS);
}

/// Every refusal the stand-in provider gives, as the provider builds it.
fn refusals() -> Vec<bmo_chat::ChatRefusal> {
  use bmo_chat::{ChatCapability, ChatRefusal};
  let label = || "Stand-in".to_owned();
  let missed = on_provider::sents().into_iter().flat_map(|sent| sent.missed).collect();
  vec![
    ChatRefusal::UnsupportedCapability {
      label: label(),
      capability: ChatCapability::Sticker,
    },
    ChatRefusal::NoPictureSent {
      label: label(),
      missed,
    },
    ChatRefusal::SpeakerNotAllowed {
      label: label(),
      agent: "helper".into(),
    },
    ChatRefusal::SpeakerUnnumbered {
      label: label(),
      agent: "helper".into(),
    },
    ChatRefusal::NotReady { label: label() },
    ChatRefusal::Platform {
      label: label(),
      detail: "That message cannot be pinned.".into(),
    },
    ChatRefusal::Gone {
      label: label(),
      detail: "That message can no longer be edited.".into(),
    },
  ]
}

/// What the stand-in provider's sends come to, in turn: every performed send, then every refusal.
fn send_outcomes() -> Vec<bmo_chat::ChatSendResult> {
  use bmo_chat::ChatSendResult;
  let sent = on_provider::sents().into_iter().map(ChatSendResult::Sent);
  sent.chain(refusals().into_iter().map(ChatSendResult::Refused)).collect()
}

/// What the stand-in provider's fetches come to, in turn: the file, the file withheld, and two refusals.
fn media_outcomes() -> Vec<bmo_chat::ChatMediaResult> {
  use bmo_chat::ChatMediaResult;
  let mut refusals = refusals();
  vec![
    ChatMediaResult::Fetched(on_provider::fetched()),
    ChatMediaResult::Unavailable {
      reason: "The platform no longer keeps that file.".into(),
    },
    ChatMediaResult::Refused(refusals.remove(0)),
    ChatMediaResult::Refused(refusals.pop().expect("a refusal that the message is gone")),
  ]
}

/// The provider's error shape as the client reads it, with the status the client expects for its code.
fn as_the_client_reads(error: bmo_chat::WireError) -> (u16, bmo_chat_client::WireError) {
  let read: bmo_chat_client::WireError =
    serde_json::from_value(serde_json::to_value(error).unwrap()).expect("the client reads the error shape");
  (read.code.status(), read)
}

/// The client's failure as the status and the error shape it was answered with.
fn answered_with(failure: bmo_chat_client::ClientFailure) -> (u16, bmo_chat_client::WireError) {
  match failure {
    bmo_chat_client::ClientFailure::Answered {
      status,
      answer: Some(answer),
      ..
    } => (status, answer),
    other => panic!("the router answered in the error shape, so the client says so: {other:?}"),
  }
}

/// A waiter for both crates' patience that fails the test if it is ever asked to wait. The patience every
/// test gives offers a request once, so nothing waits on the real clock.
struct NeverWaits;

#[async_trait]
impl bmo_chat::ChatWait for NeverWaits {
  async fn wait(&self, interval: Duration) {
    panic!("a request was offered again after {interval:?}");
  }
}

#[async_trait]
impl bmo_chat_client::ChatWait for NeverWaits {
  async fn wait(&self, interval: Duration) {
    panic!("a request was offered again after {interval:?}");
  }
}

fn client_to(base: String, token: &str) -> bmo_chat_client::ChatProviderClient {
  bmo_chat_client::ChatProviderClient::new(base, token).with_patience(bmo_chat_client::ChatPatience {
    window: Duration::ZERO,
    every: Duration::ZERO,
    waiter: Arc::new(NeverWaits),
  })
}

fn link_to(base: String, token: &str) -> bmo_chat::ChatReceiverLink {
  bmo_chat::ChatReceiverLink::new(base, token).with_patience(bmo_chat::ChatPatience {
    window: Duration::ZERO,
    every: Duration::ZERO,
    waiter: Arc::new(NeverWaits),
  })
}

/// A server on the loopback, until it is closed.
struct Listening {
  address: SocketAddr,
  stop: oneshot::Sender<()>,
  served: JoinHandle<()>,
}

impl Listening {
  async fn start(router: Router) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("the loopback takes a listener");
    let address = listener.local_addr().expect("a bound listener has an address");
    let (stop, stopped) = oneshot::channel::<()>();
    let served = tokio::spawn(async move {
      axum::serve(listener, router)
        .with_graceful_shutdown(async {
          stopped.await.ok();
        })
        .await
        .expect("the server runs");
    });
    Self {
      address,
      stop,
      served,
    }
  }

  /// The base address of what is mounted under `prefix`, written with a closing slash as a configuration
  /// might give it.
  fn base(&self, prefix: &str) -> String {
    format!("http://{}/{prefix}/", self.address)
  }

  async fn close(self) {
    self.stop.send(()).ok();
    self.served.await.expect("the server stops cleanly");
  }
}

/// A stand-in dialect: its declaration is the stand-in provider's fixture, and it reads and checks the
/// fixtures' texts as the fixtures say.
struct StandDialect {
  declared: bmo_chat::ChatDialectDeclaration,
}

impl StandDialect {
  fn reading(markdown: &str) -> bmo_chat::DialectReading {
    let index = on_provider::dialect_reads()
      .iter()
      .position(|read| read.text == markdown)
      .unwrap_or_else(|| panic!("the dialect was asked to read a text no fixture has: {markdown:?}"));
    on_provider::dialect_readings().swap_remove(index)
  }

  fn checked(markup: &str) -> bmo_chat::DialectChecked {
    let index = on_provider::dialect_checks()
      .iter()
      .position(|check| check.markup == markup)
      .unwrap_or_else(|| panic!("the dialect was asked to check markup no fixture has: {markup:?}"));
    on_provider::dialect_checkeds().swap_remove(index)
  }
}

impl bmo_chat::ChatDialect for StandDialect {
  fn name(&self) -> &str {
    &self.declared.name
  }

  fn limits(&self) -> bmo_chat::ChatDialectLimits {
    self.declared.limits
  }

  fn writing(&self) -> &str {
    &self.declared.writing
  }

  fn foldable(&self) -> bool {
    self.declared.foldable
  }

  fn convert(&self, markdown: &str) -> bmo_chat::ChatMarkup {
    Self::reading(markdown).markup
  }

  fn parts(&self, markdown: &str) -> Vec<String> {
    Self::reading(markdown).parts
  }

  fn fault(&self, markup: &str) -> Option<String> {
    Self::checked(markup).fault
  }

  fn fold(&self, markdown: &str) -> Option<String> {
    Self::reading(markdown).fold
  }

  fn visible(&self, markup: &str) -> String {
    Self::checked(markup).visible
  }
}

/// A stand-in provider that declares itself as the first declaration fixture. It answers each send and
/// fetch with the next result fixture, each room route with the fixture of the room asked about, and keeps
/// everything it was asked.
struct Stand {
  declared: bmo_chat::ChatDeclaration,
  dialect: StandDialect,
  configured: Mutex<Option<String>>,
  failing: AtomicBool,
  sends: Mutex<Vec<bmo_chat::ChatSendRequest>>,
  fetches: Mutex<Vec<bmo_chat::ChatMediaRequest>>,
  rooms: Mutex<Vec<(&'static str, i64)>>,
  owner_rooms: AtomicUsize,
}

impl Stand {
  fn new() -> Self {
    let declared = on_provider::declarations().swap_remove(0);
    Self {
      dialect: StandDialect {
        declared: declared.dialect.clone(),
      },
      configured: Mutex::new(declared.configured.clone()),
      declared,
      failing: AtomicBool::new(false),
      sends: Mutex::default(),
      fetches: Mutex::default(),
      rooms: Mutex::default(),
      owner_rooms: AtomicUsize::new(0),
    }
  }

  /// Keeps the room route asked, and gives the index of the room among the room fixtures.
  fn room(&self, route: &'static str, chat_id: i64) -> usize {
    self.rooms.lock().unwrap().push((route, chat_id));
    on_provider::room_queries()
      .iter()
      .position(|query| query.chat_id == chat_id)
      .unwrap_or_else(|| panic!("the provider was asked about a room no fixture has: {chat_id}"))
  }
}

#[async_trait]
impl bmo_chat::ChatProvider for Stand {
  fn key(&self) -> &str {
    &self.declared.key
  }

  fn label(&self) -> &str {
    &self.declared.label
  }

  fn configured(&self) -> Option<String> {
    self.configured.lock().unwrap().clone()
  }

  async fn start(&self, _receiver: Arc<dyn bmo_chat::ChatReceiver>) -> Result<(), bmo_chat::ChatFailure> {
    Ok(())
  }

  async fn stop(&self) {}

  fn modes(&self) -> Vec<bmo_chat::ChatReadMode> {
    self.declared.modes.clone()
  }

  fn can(&self) -> BTreeSet<bmo_chat::ChatCapability> {
    self.declared.can.clone()
  }

  fn dialect(&self) -> &dyn bmo_chat::ChatDialect {
    &self.dialect
  }

  fn shows(&self) -> Option<String> {
    self.declared.shows.clone()
  }

  fn numbering(&self) -> bmo_chat::ChatNumbering {
    self.declared.numbering
  }

  fn speakers(&self) -> bool {
    self.declared.speakers
  }

  async fn owns_room(&self, chat_id: i64) -> Result<bool, bmo_chat::ChatFailure> {
    Ok(on_provider::room_owns()[self.room("owns", chat_id)].owns)
  }

  async fn identity_of(&self, chat_id: i64) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    Ok(on_provider::room_identities()[self.room("identity", chat_id)].identity)
  }

  async fn is_owner_room(&self, chat_id: i64) -> Result<bmo_chat::RoomKind, bmo_chat::ChatFailure> {
    Ok(on_provider::room_kinds()[self.room("is-owner", chat_id)].kind)
  }

  async fn private_room_of(&self, chat_id: i64) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    Ok(on_provider::room_persons()[self.room("private-of", chat_id)].person)
  }

  async fn owner_room(&self) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    let rooms = on_provider::owner_rooms();
    Ok(rooms[self.owner_rooms.fetch_add(1, Ordering::SeqCst) % rooms.len()].chat_id)
  }

  async fn room_title(&self, chat_id: i64) -> Result<Option<String>, bmo_chat::ChatFailure> {
    Ok(on_provider::room_titles().swap_remove(self.room("title", chat_id)).title)
  }

  async fn send(
    &self,
    request: bmo_chat::ChatSendRequest,
  ) -> Result<bmo_chat::ChatSendResult, bmo_chat::ChatFailure> {
    if self.failing.load(Ordering::SeqCst) {
      return Err("The platform dropped the connection.".into());
    }
    let mut sends = self.sends.lock().unwrap();
    let mut results = send_outcomes();
    let answer = results.swap_remove(sends.len() % results.len());
    sends.push(request);
    Ok(answer)
  }

  async fn avatars(&self) -> Result<Vec<bmo_chat::ChatAvatar>, bmo_chat::ChatFailure> {
    let mut kept = on_provider::avatars();
    kept.reverse();
    Ok(kept)
  }

  async fn avatar(
    &self,
    kind: bmo_chat::ChatAvatarKind,
    id: i64,
  ) -> Result<Option<bmo_chat::ChatPicture>, bmo_chat::ChatFailure> {
    Ok((kind == bmo_chat::ChatAvatarKind::Room && id == -100).then(kept_picture))
  }

  async fn fetch_media(
    &self,
    request: bmo_chat::ChatMediaRequest,
  ) -> Result<bmo_chat::ChatMediaResult, bmo_chat::ChatFailure> {
    let mut fetches = self.fetches.lock().unwrap();
    let mut results = media_outcomes();
    let answer = results.swap_remove(fetches.len() % results.len());
    fetches.push(request);
    Ok(answer)
  }
}

/// The stand-in provider behind the real provider router, mounted under a prefix on the loopback.
async fn serve_stand() -> (Arc<Stand>, Listening) {
  let stand = Arc::new(Stand::new());
  let router = Router::new().nest("/provider", bmo_chat::provider_router(stand.clone(), TOKEN));
  (stand, Listening::start(router).await)
}

#[tokio::test]
async fn every_provider_route_is_served_by_the_real_router_and_answered_to_the_real_client() {
  let (stand, listening) = serve_stand().await;
  let client = client_to(listening.base("provider"), TOKEN);

  assert_eq!(client.declaration().await.unwrap(), on_client::declarations().swap_remove(0));
  assert_eq!(client.health().await.unwrap(), on_client::healths().swap_remove(0));

  let outcomes = send_outcomes();
  assert!(on_client::sends().len() >= outcomes.len(), "every outcome is answered at least once");
  for (index, request) in on_client::sends().iter().enumerate() {
    let result = client.send(request).await;
    match &outcomes[index % outcomes.len()] {
      bmo_chat::ChatSendResult::Sent(_) => {
        let sent = on_client::sents().swap_remove(index % outcomes.len());
        assert_eq!(result.unwrap(), sent, "send {index}");
      }
      bmo_chat::ChatSendResult::Refused(refusal) => assert_eq!(
        answered_with(result.unwrap_err()),
        as_the_client_reads(refusal.clone().into()),
        "send {index}"
      ),
    }
  }
  assert_eq!(*stand.sends.lock().unwrap(), on_provider::sends());

  let requests = on_client::media_requests();
  let outcomes = media_outcomes();
  let fetched = requests.len().max(outcomes.len());
  for index in 0..fetched {
    let result = client.media(&requests[index % requests.len()]).await;
    let refused = match &outcomes[index % outcomes.len()] {
      bmo_chat::ChatMediaResult::Fetched(_) => {
        assert_eq!(result.unwrap(), on_client::fetched(), "media {index}");
        continue;
      }
      bmo_chat::ChatMediaResult::Unavailable { reason } => {
        bmo_chat::WireError::new(bmo_chat::ErrorCode::Unavailable, reason.clone())
      }
      bmo_chat::ChatMediaResult::Refused(refusal) => refusal.clone().into(),
    };
    assert_eq!(answered_with(result.unwrap_err()), as_the_client_reads(refused), "media {index}");
  }
  let provider_requests = on_provider::media_requests();
  let heard: Vec<_> =
    (0..fetched).map(|index| provider_requests[index % provider_requests.len()]).collect();
  assert_eq!(*stand.fetches.lock().unwrap(), heard);

  let mut asked = Vec::new();
  for (index, query) in on_client::room_queries().into_iter().enumerate() {
    let chat_id = query.chat_id;
    assert_eq!(client.room_owns(chat_id).await.unwrap(), on_client::room_owns()[index].owns);
    assert_eq!(
      client.room_identity(chat_id).await.unwrap(),
      on_client::room_identities()[index].identity
    );
    assert_eq!(client.room_kind(chat_id).await.unwrap(), on_client::room_kinds()[index].kind);
    assert_eq!(client.room_private_of(chat_id).await.unwrap(), on_client::room_persons()[index].person);
    assert_eq!(
      client.room_title(chat_id).await.unwrap(),
      on_client::room_titles().swap_remove(index).title
    );
    let chat_id = on_provider::room_queries()[index].chat_id;
    asked.extend(["owns", "identity", "is-owner", "private-of", "title"].map(|route| (route, chat_id)));
  }
  assert_eq!(*stand.rooms.lock().unwrap(), asked);
  for answer in on_client::owner_rooms() {
    assert_eq!(client.owner_room().await.unwrap(), answer.chat_id);
  }

  for (read, reading) in on_client::dialect_reads().into_iter().zip(on_client::dialect_readings()) {
    assert_eq!(client.dialect_read(read.text).await.unwrap(), reading);
  }
  for (check, checked) in on_client::dialect_checks().into_iter().zip(on_client::dialect_checkeds()) {
    assert_eq!(client.dialect_check(check.markup).await.unwrap(), checked);
  }

  drop(client);
  listening.close().await;
}

#[tokio::test]
async fn the_real_routers_errors_reach_the_real_client_in_the_error_shape() {
  use bmo_chat_client::{ChatHealth, ClientFailure, ErrorCode, WireError};

  let (stand, listening) = serve_stand().await;
  let client = client_to(listening.base("provider"), TOKEN);
  let answered = |failure: ClientFailure| match failure {
    ClientFailure::Answered { status, answer, .. } => (status, answer),
    other => panic!("the router answered, so the client says so: {other:?}"),
  };
  let send = on_client::sends().swap_remove(0);

  let stranger = client_to(listening.base("provider"), "a-wrong-token");
  let (status, answer) = answered(stranger.send(&send).await.unwrap_err());
  assert_eq!(status, 401);
  assert_eq!(
    answer,
    Some(WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong."))
  );

  *stand.configured.lock().unwrap() = Some("The platform connection is not open yet.".into());
  assert_eq!(client.health().await.unwrap(), on_client::healths().swap_remove(1));
  let (status, answer) = answered(client.send(&send).await.unwrap_err());
  assert_eq!(status, 409);
  let not_ready = "Stand-in is not ready yet: The platform connection is not open yet.";
  assert_eq!(answer, Some(WireError::new(ErrorCode::NotReady, not_ready)));
  let (status, _) = answered(client.room_owns(7).await.unwrap_err());
  assert_eq!(status, 409);

  *stand.configured.lock().unwrap() = None;
  assert_eq!(client.health().await.unwrap(), ChatHealth::ready());
  stand.failing.store(true, Ordering::SeqCst);
  let (status, answer) = answered(client.send(&send).await.unwrap_err());
  assert_eq!(status, 500);
  assert_eq!(answer, Some(WireError::new(ErrorCode::Failed, "The platform dropped the connection.")));
  assert!(stand.sends.lock().unwrap().is_empty(), "no send reached the provider's own sending");
  assert!(
    stand.rooms.lock().unwrap().is_empty(),
    "no room question reached a provider that was not ready"
  );

  drop((client, stranger));
  listening.close().await;
}

/// The picture the stand-in provider keeps for room -100.
fn kept_picture() -> bmo_chat::ChatPicture {
  bmo_chat::ChatPicture {
    mime: "image/jpeg".into(),
    bytes: vec![0xff, 0xd8, 0xff],
  }
}

#[tokio::test]
async fn the_avatar_routes_are_served_by_the_real_router_and_read_by_the_real_client() {
  use bmo_chat_client::{ChatAvatarKind, ChatPicture, ClientFailure, ErrorCode, WireError};

  let (_stand, listening) = serve_stand().await;
  let client = client_to(listening.base("provider"), TOKEN);
  for (query, list) in on_client::avatar_queries()
    .iter()
    .zip([on_client::avatars(), on_client::avatars().split_off(1)])
  {
    assert_eq!(client.avatars(query).await.unwrap(), list, "{query:?}");
  }
  assert_eq!(
    client.avatar(ChatAvatarKind::Room, -100).await.unwrap(),
    Some(ChatPicture {
      mime: "image/jpeg".into(),
      bytes: vec![0xff, 0xd8, 0xff],
    })
  );
  assert_eq!(client.avatar(ChatAvatarKind::Person, 42).await.unwrap(), None);

  let http = reqwest::Client::builder().no_proxy().build().expect("a plain client builds");
  let base = listening.base("provider");
  let answer = http
    .get(format!("{base}avatar/person/9"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .expect("the router answers");
  assert_eq!(answer.status().as_u16(), 404);
  let body = answer.bytes().await.expect("the answer has a body");
  let read: WireError = serde_json::from_slice(&body).expect("the body is the error shape");
  assert_eq!(read, WireError::new(ErrorCode::NoRoute, "There is no route at GET /avatar/person/9."));
  let stranger = client_to(listening.base("provider"), "a-wrong-token");
  let failure = stranger.avatar(ChatAvatarKind::Room, -100).await.unwrap_err();
  assert!(matches!(failure, ClientFailure::Answered { status: 401, .. }), "{failure:?}");

  drop((client, stranger, http));
  listening.close().await;
}

/// What the stand-in receiver took, by the route it came in on.
#[derive(Debug, Clone, PartialEq)]
enum Took {
  Message(bmo_chat_client::ChatHandover),
  Attachment(bmo_chat_client::ChatHandover),
  Edited(bmo_chat_client::ChatHandover),
  Gathered(bmo_chat_client::ChatGathered),
  History(bmo_chat_client::ChatHandover),
  AvatarsUpdated(bmo_chat_client::AvatarsUpdated),
  Listened(bmo_chat_client::ListenedQuery),
  RoomsOf(bmo_chat_client::RoomsOfQuery),
}

/// A stand-in receiver that keeps every hand-over, or answers each with the error it is set to give.
#[derive(Default)]
struct Taker {
  took: Mutex<Vec<Took>>,
  refusing: Mutex<Option<bmo_chat_client::WireError>>,
}

impl Taker {
  fn take(&self, took: Took) -> Result<(), bmo_chat_client::WireError> {
    if let Some(error) = self.refusing.lock().unwrap().clone() {
      return Err(error);
    }
    self.took.lock().unwrap().push(took);
    Ok(())
  }
}

#[async_trait]
impl bmo_chat_client::ChatReceiver for Taker {
  async fn message(
    &self,
    handover: bmo_chat_client::ChatHandover,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::Message(handover))
  }

  async fn attachment(
    &self,
    handover: bmo_chat_client::ChatHandover,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::Attachment(handover))
  }

  async fn edited(
    &self,
    handover: bmo_chat_client::ChatHandover,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::Edited(handover))
  }

  async fn gathered(
    &self,
    gathered: bmo_chat_client::ChatGathered,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::Gathered(gathered))
  }

  /// Answers each history hand-over with the answer fixture of its place among the hand-over fixtures.
  async fn history(
    &self,
    handover: bmo_chat_client::ChatHandover,
  ) -> Result<bool, bmo_chat_client::WireError> {
    let at = on_client::handovers()
      .iter()
      .position(|fixture| *fixture == handover)
      .expect("the history hand-over is a fixture");
    self.take(Took::History(handover))?;
    Ok(on_client::history_kept()[at].kept)
  }

  async fn avatars_updated(
    &self,
    updated: bmo_chat_client::AvatarsUpdated,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::AvatarsUpdated(updated))
  }

  async fn listened(
    &self,
    query: bmo_chat_client::ListenedQuery,
  ) -> Result<bool, bmo_chat_client::WireError> {
    let at = on_client::listened_queries()
      .iter()
      .position(|fixture| *fixture == query)
      .expect("the room asked about is a fixture");
    self.take(Took::Listened(query))?;
    Ok(on_client::listened()[at].listened)
  }

  async fn rooms_of(
    &self,
    query: bmo_chat_client::RoomsOfQuery,
  ) -> Result<Vec<bmo_chat_client::HistoryRoom>, bmo_chat_client::WireError> {
    let at = on_client::rooms_of_queries()
      .iter()
      .position(|fixture| *fixture == query)
      .expect("the query is a fixture");
    self.take(Took::RoomsOf(query))?;
    Ok(on_client::rooms_of().swap_remove(at).rooms)
  }
}

#[tokio::test]
async fn every_inbound_route_is_posted_by_the_real_link_into_the_real_receiver_router() {
  use bmo_chat::ChatReceiver;

  let taker = Arc::new(Taker::default());
  let router = Router::new().nest("/receiver", bmo_chat_client::receiver_router(taker.clone(), TOKEN));
  let listening = Listening::start(router).await;
  let link = link_to(listening.base("receiver"), TOKEN);

  for handover in on_provider::handovers() {
    link.message(handover.clone()).await.unwrap();
    link.attachment(handover.clone()).await.unwrap();
    link.edited(handover).await.unwrap();
  }
  for gathered in on_provider::gathered() {
    link.gathered(gathered).await.unwrap();
  }
  for (handover, kept) in on_provider::handovers().into_iter().zip(on_provider::history_kept()) {
    assert_eq!(link.history(handover).await.unwrap(), kept.kept);
  }
  for updated in on_provider::avatars_updated() {
    link.avatars_updated(updated).await.unwrap();
  }
  for (query, answer) in on_provider::listened_queries().into_iter().zip(on_provider::listened()) {
    assert_eq!(link.listened(query).await.unwrap(), answer.listened);
  }
  for (query, answer) in on_provider::rooms_of_queries().into_iter().zip(on_provider::rooms_of()) {
    assert_eq!(link.rooms_of(query).await.unwrap(), answer.rooms);
  }
  let mut expected = Vec::new();
  for handover in on_client::handovers() {
    expected.extend([
      Took::Message(handover.clone()),
      Took::Attachment(handover.clone()),
      Took::Edited(handover),
    ]);
  }
  expected.extend(on_client::gathered().into_iter().map(Took::Gathered));
  expected.extend(on_client::handovers().into_iter().map(Took::History));
  expected.extend(on_client::avatars_updated().into_iter().map(Took::AvatarsUpdated));
  expected.extend(on_client::listened_queries().into_iter().map(Took::Listened));
  expected.extend(on_client::rooms_of_queries().into_iter().map(Took::RoomsOf));
  assert_eq!(*taker.took.lock().unwrap(), expected);

  let given = on_client::wire_errors();
  for (index, error) in on_provider::wire_errors().into_iter().enumerate() {
    if !matches!(error.code, bmo_chat::ErrorCode::NotReady | bmo_chat::ErrorCode::Failed) {
      continue;
    }
    *taker.refusing.lock().unwrap() = Some(given[index].clone());
    let failure = link.message(on_provider::handovers().swap_remove(0)).await.unwrap_err();
    match failure.downcast_ref::<bmo_chat::LinkFailure>() {
      Some(bmo_chat::LinkFailure::Answered { status, answer, .. }) => {
        assert_eq!(*status, error.code.status());
        assert_eq!(answer.as_ref(), Some(&error));
      }
      other => panic!("the receiver answered, so the link says so: {other:?}"),
    }
  }
  assert_eq!(taker.took.lock().unwrap().len(), expected.len(), "a refused hand-over is not taken");

  let stranger = link_to(listening.base("receiver"), "a-wrong-token");
  let failure = stranger.edited(on_provider::handovers().swap_remove(0)).await.unwrap_err();
  match failure.downcast_ref::<bmo_chat::LinkFailure>() {
    Some(bmo_chat::LinkFailure::Answered { status, answer, .. }) => {
      assert_eq!(*status, 401);
      assert_eq!(answer.as_ref().map(|answer| answer.code), Some(bmo_chat::ErrorCode::Unauthorized));
    }
    other => panic!("the receiver answered, so the link says so: {other:?}"),
  }

  drop((link, stranger));
  listening.close().await;
}
