//! The two base libraries agree on the wire. Every piece both crates define crosses from one crate's
//! encoder into the other's decoder, both ways. Over the loopback, the real provider router serves a
//! stand-in provider to the real provider client, and the real link calls the real receiving-side router. No
//! test waits on the real clock, and every listener a test starts is taken down before it ends.

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

/// The room and number each handed-over message fixture is handed over under, in the fixtures' order.
const KEYS: [(i64, i64); 5] = [(-100, 1), (7, 2), (-100, 3), (7, 4), (-100, 5)];

/// The place among the handed-over message fixtures of the one still gathering its files.
const GATHERING: usize = 3;

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

      fn handed(origin: MessageOrigin, text: &str) -> HandedMessage {
        HandedMessage {
          origin,
          sender_id: None,
          sender_name: "A person".into(),
          sent_at: 1_760_000_000_000,
          text: text.into(),
          reply_to_message_id: None,
          via: "stand-in".into(),
          album: None,
          mode: ChatVoice::BotAccount,
          context: InboundChatContext::default(),
          room: RoomFacts::default(),
          files: vec![],
          gathering: false,
          gather_failure: None,
        }
      }

      fn label() -> String {
        "Stand-in".into()
      }

      /// Every kind of send, each with every field it takes set and, where it has any, with none of its
      /// optional fields; every voice speaks one of them.
      pub fn sends() -> Vec<ChatSendRequest> {
        vec![
          ChatSendRequest {
            room_id: -100,
            send: ChatSend::Text {
              text: "Hello **there**".into(),
              silent: true,
              fold: true,
            },
            mode: Some(ChatVoice::HumanAvatar),
            speaker: Some("helper".into()),
            tag: Some("send-1".into()),
            prefix: Some("[helper] ".into()),
            reply_to_message_id: Some(41),
          },
          ChatSendRequest::new(
            7,
            ChatSend::Text {
              text: "Just words".into(),
              silent: false,
              fold: false,
            },
          ),
          ChatSendRequest {
            mode: Some(ChatVoice::BotAccount),
            reply_to_message_id: Some(42),
            ..ChatSendRequest::new(
              -100,
              ChatSend::Photo {
                image_url: "https://example.com/picture.jpg".into(),
                caption: Some("A picture".into()),
              },
            )
          },
          ChatSendRequest::new(
            7,
            ChatSend::Photo {
              image_url: "/data/picture.png".into(),
              caption: None,
            },
          ),
          ChatSendRequest {
            mode: Some(ChatVoice::HumanReader),
            tag: Some("album-1".into()),
            reply_to_message_id: Some(43),
            ..ChatSendRequest::new(
              -100,
              ChatSend::Album {
                items: vec![
                  AlbumItem::Address {
                    address: "https://example.com/one.jpg".into(),
                  },
                  AlbumItem::File(file("two.png", "image/png", &[0x89, b'P', b'N', b'G', 0, 255])),
                ],
                caption: Some("Two pictures".into()),
              },
            )
          },
          ChatSendRequest::new(
            7,
            ChatSend::Album {
              items: vec![AlbumItem::Address {
                address: "/data/one.jpg".into(),
              }],
              caption: None,
            },
          ),
          ChatSendRequest {
            reply_to_message_id: Some(44),
            ..ChatSendRequest::new(
              7,
              ChatSend::Document {
                bytes: b"%PDF-1.7 a document".to_vec(),
                file_name: "notes.pdf".into(),
              },
            )
          },
          ChatSendRequest::new(
            7,
            ChatSend::Audio {
              audio: vec![0xff, 0xfb, 0x90, 0x00],
              file_name: "song.mp3".into(),
            },
          ),
          ChatSendRequest {
            mode: Some(ChatVoice::OwnAccount),
            reply_to_message_id: Some(45),
            ..ChatSendRequest::new(
              7,
              ChatSend::Voice {
                audio: vec![b'O', b'g', b'g', b'S', 0],
                said: "Good morning".into(),
                caption: Some("A greeting".into()),
                transcript: Some("Good morning!".into()),
              },
            )
          },
          ChatSendRequest::new(
            7,
            ChatSend::Voice {
              audio: vec![1, 2, 3],
              said: "Bye".into(),
              caption: None,
              transcript: None,
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
            mode: Some(ChatVoice::Direct),
            ..ChatSendRequest::new(0, ChatSend::Typing { on: true })
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

      /// The messages whose files are fetched again, by the path and query of the route.
      pub fn media_requests() -> Vec<ChatMediaRequest> {
        vec![
          ChatMediaRequest {
            room_id: -100,
            message_id: 60,
            sent_at: Some(1_760_000_100_000),
          },
          ChatMediaRequest {
            room_id: 7,
            message_id: 61,
            sent_at: None,
          },
        ]
      }

      pub fn files_queries() -> Vec<FilesQuery> {
        media_requests().into_iter().map(|request| FilesQuery { sent_at: request.sent_at }).collect()
      }

      pub fn fetched() -> Vec<MessageFile> {
        vec![
          message_file("photo-5", "picture.jpg", "image/jpeg", &[0xff, 0xd8, 0xff, 0xe0]),
          message_file("photo-6", "picture.png", "image/png", &[0x89, b'P']),
        ]
      }

      pub fn file_pages() -> Vec<Page<MessageFile>> {
        vec![
          Page {
            items: fetched(),
            more: false,
          },
          Page {
            items: kinded_files(),
            more: false,
          },
        ]
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
                voice: ChatVoice::OwnAccount,
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
            version: "1.0.0".into(),
          },
          ChatHealth::not_ready("1.0.0", "The platform connection is not open yet."),
        ]
      }

      /// The rooms the stand-in provider knows, one of each kind.
      pub fn provider_rooms() -> Vec<ProviderRoom> {
        vec![
          ProviderRoom {
            id: -100,
            owns: true,
            identity: Some(7001),
            kind: RoomKind::Group,
            private_to: None,
            title: Some("The garden".into()),
          },
          ProviderRoom {
            id: 7,
            owns: false,
            identity: None,
            kind: RoomKind::Owner,
            private_to: Some(42),
            title: None,
          },
        ]
      }

      /// The queries for the rooms of the account, and below, the page answering each in the same order.
      pub fn account_room_queries() -> Vec<AccountRoomsQuery> {
        vec![
          AccountRoomsQuery::default(),
          AccountRoomsQuery {
            after: Some(-100),
            limit: None,
          },
          AccountRoomsQuery {
            after: None,
            limit: Some(1),
          },
        ]
      }

      pub fn account_room_pages() -> Vec<Page<ProviderRoom>> {
        vec![
          Page {
            items: provider_rooms(),
            more: false,
          },
          Page {
            items: provider_rooms().split_off(1),
            more: false,
          },
          Page {
            items: provider_rooms()[..1].to_vec(),
            more: true,
          },
        ]
      }

      pub fn provider_owners() -> Vec<ProviderOwner> {
        vec![ProviderOwner { room_id: Some(7) }, ProviderOwner { room_id: None }]
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

      /// The messages handed over, one of each origin, one with every field set, one carrying its files, one
      /// still gathering them and one whose gathering failed; each goes under its place in `KEYS`.
      pub fn handed_messages() -> Vec<HandedMessage> {
        vec![
          handed(MessageOrigin::Live, "Hello"),
          HandedMessage {
            sender_id: Some(42),
            sender_name: "Another person".into(),
            sent_at: 1_760_000_200_000,
            reply_to_message_id: Some(1),
            album: Some(2),
            mode: ChatVoice::HumanReader,
            context: InboundChatContext {
              room_title: Some("The garden".into()),
              sender_username: Some("another".into()),
              reply_to_sender_id: Some(7001),
              spoken_text: Some("Look at this".into()),
              metadata: Some(json!({ "platform": { "kind": "photo", "sizes": [320, 1280] } })),
              carries_file: true,
              tag: Some("send-1".into()),
            },
            room: RoomFacts {
              kind: Some(RoomKind::Owner),
              identity: Some(7001),
              private_to: Some(42),
            },
            ..handed(MessageOrigin::Edit, "[picture]")
          },
          HandedMessage {
            context: InboundChatContext {
              carries_file: true,
              ..InboundChatContext::default()
            },
            room: RoomFacts {
              kind: Some(RoomKind::Group),
              identity: None,
              private_to: None,
            },
            files: vec![
              message_file("0", "notes.txt", "text/plain", b"some notes"),
              message_file("1", "tiny.png", "image/png", &[0x89, b'P', b'N', b'G']),
            ],
            ..handed(MessageOrigin::History, "Said long ago")
          },
          HandedMessage {
            gathering: true,
            ..handed(MessageOrigin::Live, "[file]")
          },
          HandedMessage {
            gather_failure: Some("The file is larger than the platform hands out.".into()),
            ..handed(MessageOrigin::Live, "[file]")
          },
        ]
      }

      pub fn gathered_patches() -> Vec<GatheredPatch> {
        vec![
          GatheredPatch::kept(vec![message_file("photo-2", "picture.jpg", "image/jpeg", &[0xff, 0xd8, 0xff])]),
          GatheredPatch::kept(kinded_files()),
          GatheredPatch::not_kept("The file is larger than the platform hands out."),
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

      /// The pictures the stand-in provider keeps, in the order a page answers them.
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

      /// The queries for pictures, and below, the page answering each in the same order.
      pub fn avatar_queries() -> Vec<AvatarsQuery> {
        vec![
          AvatarsQuery::default(),
          AvatarsQuery {
            after: Some(1_760_000_000_000),
            limit: Some(1),
          },
          AvatarsQuery {
            after: None,
            limit: Some(1),
          },
        ]
      }

      pub fn avatar_pages() -> Vec<Page<ChatAvatar>> {
        vec![
          Page {
            items: avatars(),
            more: false,
          },
          Page {
            items: avatars().split_off(1),
            more: false,
          },
          Page {
            items: avatars()[..1].to_vec(),
            more: true,
          },
        ]
      }

      pub fn avatar_changes() -> Vec<AvatarChange> {
        vec![AvatarChange {
          via: "stand-in".into(),
          room_ids: vec![-100],
          person_ids: vec![42, 43],
        }]
      }

      pub fn room_queries() -> Vec<RoomQuery> {
        vec![RoomQuery {
          via: "stand-in".into(),
        }]
      }

      /// The rooms the stand-in receiving side holds, in ascending order of number.
      pub fn receiver_rooms() -> Vec<ReceiverRoom> {
        vec![
          ReceiverRoom {
            id: -100,
            listened: true,
            oldest_message_id: Some(1),
            oldest_sent_at: Some(1_760_000_000_000),
          },
          ReceiverRoom {
            id: 7,
            listened: false,
            oldest_message_id: None,
            oldest_sent_at: None,
          },
        ]
      }

      /// The queries for rooms to read back, and below, the page answering each in the same order.
      pub fn rooms_queries() -> Vec<RoomsQuery> {
        vec![
          RoomsQuery {
            via: "stand-in".into(),
            after: None,
            limit: None,
          },
          RoomsQuery {
            via: "stand-in".into(),
            after: Some(-100),
            limit: Some(1),
          },
          RoomsQuery {
            via: "stand-in".into(),
            after: None,
            limit: Some(1),
          },
        ]
      }

      pub fn room_pages() -> Vec<Page<ReceiverRoom>> {
        vec![
          Page {
            items: receiver_rooms(),
            more: false,
          },
          Page {
            items: receiver_rooms().split_off(1),
            more: false,
          },
          Page {
            items: receiver_rooms()[..1].to_vec(),
            more: true,
          },
        ]
      }

      /// The error shape with every code, the refusals' among them.
      pub fn wire_errors() -> Vec<WireError> {
        vec![
          WireError::new(ErrorCode::BadRequest, "The body is not the shape this route takes."),
          WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong."),
          WireError::new(ErrorCode::Forbidden, "This token may not do that."),
          WireError::new(ErrorCode::NoRoute, "There is no route at GET /nowhere."),
          WireError::new(ErrorCode::NotFound, "No profile picture is kept for room 7."),
          WireError::new(ErrorCode::Conflict, "The message promised no files."),
          WireError::new(ErrorCode::Gone, "Stand-in: That message can no longer be edited."),
          WireError::new(ErrorCode::TooLarge, "The body is over the route's ceiling."),
          WireError::new(ErrorCode::Failed, "The archive could not be written."),
          WireError::new(ErrorCode::UpstreamFailed, "The store behind the receiving side failed."),
          WireError::new(ErrorCode::NotReady, "The receiving side is still starting."),
          WireError::new(ErrorCode::Unavailable, "The platform no longer keeps that file."),
          WireError::new(ErrorCode::UnsupportedCapability, "Stand-in cannot send a sticker."),
          WireError::new(ErrorCode::NoPictureSent, "Stand-in: no picture was sent."),
          WireError::new(ErrorCode::SpeakerNotAllowed, "Stand-in cannot send in the name of helper."),
          WireError::new(
            ErrorCode::SpeakerUnnumbered,
            "Stand-in has no number for helper, so it cannot send in that name.",
          ),
          WireError::new(ErrorCode::Platform, "Stand-in: That message cannot be pinned."),
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
  crosses("files query", on_provider::files_queries(), on_client::files_queries());
  crosses("files page", on_provider::file_pages(), on_client::file_pages());
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
  assert_eq!(bmo_chat::RETRY_AFTER_SECONDS, bmo_chat_client::RETRY_AFTER_SECONDS);
}

#[test]
fn every_declaration_health_and_picture_page_crosses_both_ways() {
  crosses("declaration", on_provider::declarations(), on_client::declarations());
  crosses("health", on_provider::healths(), on_client::healths());
  crosses("avatar page", on_provider::avatar_pages(), on_client::avatar_pages());
  crosses("avatar query", on_provider::avatar_queries(), on_client::avatar_queries());
}

#[test]
fn every_room_owner_and_dialect_body_crosses_both_ways() {
  crosses("provider room", on_provider::provider_rooms(), on_client::provider_rooms());
  crosses(
    "account rooms query",
    on_provider::account_room_queries(),
    on_client::account_room_queries(),
  );
  crosses("account rooms page", on_provider::account_room_pages(), on_client::account_room_pages());
  crosses("owner", on_provider::provider_owners(), on_client::provider_owners());
  crosses("dialect reading asked", on_provider::dialect_reads(), on_client::dialect_reads());
  crosses("dialect reading", on_provider::dialect_readings(), on_client::dialect_readings());
  crosses("dialect check asked", on_provider::dialect_checks(), on_client::dialect_checks());
  crosses("dialect check", on_provider::dialect_checkeds(), on_client::dialect_checkeds());
}

#[test]
fn every_receiving_side_body_and_the_error_shape_cross_both_ways() {
  crosses("handed message", on_provider::handed_messages(), on_client::handed_messages());
  crosses("gathered patch", on_provider::gathered_patches(), on_client::gathered_patches());
  crosses("avatar change", on_provider::avatar_changes(), on_client::avatar_changes());
  crosses("room query", on_provider::room_queries(), on_client::room_queries());
  crosses("receiving-side room", on_provider::receiver_rooms(), on_client::receiver_rooms());
  crosses("rooms query", on_provider::rooms_queries(), on_client::rooms_queries());
  crosses("rooms page", on_provider::room_pages(), on_client::room_pages());
  crosses("error", on_provider::wire_errors(), on_client::wire_errors());
}

#[test]
fn the_fixtures_hold_every_member_of_every_set() {
  use bmo_chat::{
    ChatAvatarKind, ChatCapability, ChatFileKind, ChatModeStanding, ChatNumberSpace, ChatReach, ChatVerb,
    ChatVoice, ErrorCode, MessageOrigin, RoomKind,
  };

  let kinds: BTreeSet<ChatVerb> = on_provider::sends().iter().map(|request| request.send.kind()).collect();
  assert_eq!(kinds, ChatVerb::ALL.iter().copied().collect());

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
    ChatVoice::OwnAccount,
    ChatVoice::Direct,
  ] {
    assert!(modes.iter().any(|mode| mode.voice == voice), "a mode speaks with {voice:?}");
    assert!(
      on_provider::sends().iter().any(|request| request.mode == Some(voice)),
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

  let handed = on_provider::handed_messages();
  assert_eq!(handed.len(), KEYS.len(), "every handed-over message has its key");
  assert!(handed[GATHERING].gathering, "the message at GATHERING is still gathering its files");
  for kind in [RoomKind::Owner, RoomKind::Group] {
    assert!(on_provider::provider_rooms().iter().any(|room| room.kind == kind));
    assert!(handed.iter().any(|message| message.room.kind == Some(kind)));
  }
  for origin in [MessageOrigin::Live, MessageOrigin::Edit, MessageOrigin::History] {
    assert!(handed.iter().any(|message| message.origin == origin), "a message comes {origin:?}");
  }
  assert!(handed.iter().any(|message| !message.files.is_empty()));
  assert!(handed.iter().any(|message| message.gather_failure.is_some()));
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
  assert!(on_provider::sends().iter().any(|request| request.speaker.is_some()));
  let patches = on_provider::gathered_patches();
  assert!(patches.iter().any(|patch| !patch.files.is_empty() && patch.gather_failure.is_none()));
  assert!(patches.iter().any(|patch| patch.files.is_empty() && patch.gather_failure.is_some()));
  for pages in [
    on_provider::avatar_pages().iter().map(|page| page.more).collect::<BTreeSet<_>>(),
    on_provider::room_pages().iter().map(|page| page.more).collect(),
    on_provider::account_room_pages().iter().map(|page| page.more).collect(),
  ] {
    assert_eq!(pages, BTreeSet::from([false, true]), "a page with more and one without");
  }
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
      speaker: "helper".into(),
    },
    ChatRefusal::SpeakerUnnumbered {
      label: label(),
      speaker: "helper".into(),
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

/// What the stand-in provider's fetches come to, in turn: the files, the files withheld, and two refusals.
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

/// The link's failure as the status and the error shape it was answered with.
fn link_answered_with(failure: bmo_chat::ChatFailure) -> (u16, Option<bmo_chat::WireError>) {
  match failure.downcast_ref::<bmo_chat::LinkFailure>() {
    Some(bmo_chat::LinkFailure::Answered { status, answer, .. }) => (*status, answer.clone()),
    other => panic!("the receiving side answered, so the link says so: {other:?}"),
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
/// fetch with the next result fixture, a room with the room fixture of its number, and keeps everything it
/// was asked.
struct Stand {
  declared: bmo_chat::ChatDeclaration,
  dialect: StandDialect,
  configured: Mutex<Option<String>>,
  failing: AtomicBool,
  sends: Mutex<Vec<bmo_chat::ChatSendRequest>>,
  fetches: Mutex<Vec<bmo_chat::ChatMediaRequest>>,
  rooms: Mutex<Vec<i64>>,
  owners: AtomicUsize,
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
      owners: AtomicUsize::new(0),
    }
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

  fn version(&self) -> &str {
    "1.0.0"
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

  async fn room(&self, room_id: i64) -> Result<Option<bmo_chat::ProviderRoom>, bmo_chat::ChatFailure> {
    self.rooms.lock().unwrap().push(room_id);
    Ok(on_provider::provider_rooms().into_iter().find(|room| room.id == room_id))
  }

  async fn account_rooms(&self) -> Result<Vec<bmo_chat::ProviderRoom>, bmo_chat::ChatFailure> {
    let mut listed = on_provider::provider_rooms();
    listed.reverse();
    Ok(listed)
  }

  async fn owner_room(&self) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    let owners = on_provider::provider_owners();
    Ok(owners[self.owners.fetch_add(1, Ordering::SeqCst) % owners.len()].room_id)
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
    let result = client.message_files(&requests[index % requests.len()]).await;
    let refused = match &outcomes[index % outcomes.len()] {
      bmo_chat::ChatMediaResult::Fetched(_) => {
        assert_eq!(result.unwrap(), on_client::file_pages().swap_remove(0), "files {index}");
        continue;
      }
      bmo_chat::ChatMediaResult::Unavailable { reason } => {
        bmo_chat::WireError::new(bmo_chat::ErrorCode::Unavailable, reason.clone())
      }
      bmo_chat::ChatMediaResult::Refused(refusal) => refusal.clone().into(),
    };
    assert_eq!(answered_with(result.unwrap_err()), as_the_client_reads(refused), "files {index}");
  }
  let provider_requests = on_provider::media_requests();
  let heard: Vec<_> =
    (0..fetched).map(|index| provider_requests[index % provider_requests.len()]).collect();
  assert_eq!(*stand.fetches.lock().unwrap(), heard);

  for room in on_client::provider_rooms() {
    assert_eq!(client.room(room.id).await.unwrap(), room);
  }
  let unknown = answered_with(client.room(13).await.unwrap_err());
  assert_eq!(
    unknown,
    (
      404,
      bmo_chat_client::WireError::new(bmo_chat_client::ErrorCode::NotFound, "Nothing is known of room 13.")
    )
  );
  let asked: Vec<i64> = on_provider::provider_rooms().iter().map(|room| room.id).chain([13]).collect();
  assert_eq!(*stand.rooms.lock().unwrap(), asked);
  for owner in on_client::provider_owners() {
    assert_eq!(client.owner().await.unwrap(), owner);
  }
  for (query, page) in on_client::account_room_queries().iter().zip(on_client::account_room_pages()) {
    assert_eq!(client.account_rooms(query).await.unwrap(), page, "{query:?}");
  }

  for (read, reading) in on_client::dialect_reads().into_iter().zip(on_client::dialect_readings()) {
    assert_eq!(client.dialect_reading(read.text).await.unwrap(), reading);
  }
  for (check, checked) in on_client::dialect_checks().into_iter().zip(on_client::dialect_checkeds()) {
    assert_eq!(client.dialect_check(check.markup).await.unwrap(), checked);
  }

  drop(client);
  listening.close().await;
}

#[tokio::test]
async fn the_real_routers_errors_reach_the_real_client_in_the_error_shape() {
  use bmo_chat_client::{ClientFailure, ErrorCode, WireError};

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
  assert_eq!(status, 503);
  let not_ready = "Stand-in is not ready yet: The platform connection is not open yet.";
  assert_eq!(answer, Some(WireError::new(ErrorCode::NotReady, not_ready)));
  let (status, _) = answered(client.room(7).await.unwrap_err());
  assert_eq!(status, 503);

  *stand.configured.lock().unwrap() = None;
  assert_eq!(client.health().await.unwrap(), on_client::healths().swap_remove(0));
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
  for (query, page) in on_client::avatar_queries().iter().zip(on_client::avatar_pages()) {
    assert_eq!(client.avatars(query).await.unwrap(), page, "{query:?}");
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
    .get(format!("{base}people/someone/avatar"))
    .bearer_auth(TOKEN)
    .send()
    .await
    .expect("the router answers");
  assert_eq!(answer.status().as_u16(), 404);
  let body = answer.bytes().await.expect("the answer has a body");
  let read: WireError = serde_json::from_slice(&body).expect("the body is the error shape");
  assert_eq!(
    read,
    WireError::new(ErrorCode::NoRoute, "There is no route at GET /people/someone/avatar.")
  );
  let stranger = client_to(listening.base("provider"), "a-wrong-token");
  let failure = stranger.avatar(ChatAvatarKind::Room, -100).await.unwrap_err();
  assert!(matches!(failure, ClientFailure::Answered { status: 401, .. }), "{failure:?}");

  drop((client, stranger, http));
  listening.close().await;
}

/// What the stand-in receiving side took, by the call it came in on.
#[derive(Debug, Clone, PartialEq)]
enum Took {
  HandOver(i64, i64, Box<bmo_chat_client::HandedMessage>),
  Gathered(i64, i64, bmo_chat_client::GatheredPatch),
  AvatarsChanged(bmo_chat_client::AvatarChange),
  Room(i64, bmo_chat_client::RoomQuery),
  Rooms(bmo_chat_client::RoomsQuery),
}

/// A stand-in receiving side that keeps every call and the messages it is handed, or answers each call with
/// the error it is set to give.
#[derive(Default)]
struct Taker {
  took: Mutex<Vec<Took>>,
  held: Mutex<Vec<((i64, i64), bmo_chat_client::HandedMessage)>>,
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
  /// A message held already is answered as held, and nothing else changes.
  async fn hand_over(
    &self,
    room_id: i64,
    message_id: i64,
    message: bmo_chat_client::HandedMessage,
  ) -> Result<bmo_chat_client::HeldMessage, bmo_chat_client::WireError> {
    self.take(Took::HandOver(room_id, message_id, Box::new(message.clone())))?;
    let mut held = self.held.lock().unwrap();
    if let Some((_, kept)) = held.iter().find(|(key, _)| *key == (room_id, message_id)) {
      return Ok(bmo_chat_client::HeldMessage {
        created: false,
        message: kept.clone(),
      });
    }
    held.push(((room_id, message_id), message.clone()));
    Ok(bmo_chat_client::HeldMessage {
      created: true,
      message,
    })
  }

  async fn gathered(
    &self,
    room_id: i64,
    message_id: i64,
    patch: bmo_chat_client::GatheredPatch,
  ) -> Result<bmo_chat_client::HandedMessage, bmo_chat_client::WireError> {
    self.take(Took::Gathered(room_id, message_id, patch.clone()))?;
    let held = self.held.lock().unwrap();
    let (_, kept) = held.iter().find(|(key, _)| *key == (room_id, message_id)).ok_or_else(|| {
      bmo_chat_client::WireError::new(bmo_chat_client::ErrorCode::NotFound, "No such message is held.")
    })?;
    Ok(bmo_chat_client::HandedMessage {
      files: patch.files,
      gather_failure: patch.gather_failure,
      gathering: false,
      ..kept.clone()
    })
  }

  async fn avatars_changed(
    &self,
    change: bmo_chat_client::AvatarChange,
  ) -> Result<(), bmo_chat_client::WireError> {
    self.take(Took::AvatarsChanged(change))
  }

  async fn room(
    &self,
    room_id: i64,
    query: bmo_chat_client::RoomQuery,
  ) -> Result<Option<bmo_chat_client::ReceiverRoom>, bmo_chat_client::WireError> {
    self.take(Took::Room(room_id, query))?;
    Ok(on_client::receiver_rooms().into_iter().find(|room| room.id == room_id))
  }

  async fn rooms(
    &self,
    query: bmo_chat_client::RoomsQuery,
  ) -> Result<Vec<bmo_chat_client::ReceiverRoom>, bmo_chat_client::WireError> {
    self.take(Took::Rooms(query.clone()))?;
    let mut rooms: Vec<_> = on_client::receiver_rooms()
      .into_iter()
      .filter(|room| query.after.is_none_or(|after| room.id > after))
      .collect();
    if let Some(limit) = query.limit {
      rooms.truncate(limit as usize);
    }
    Ok(rooms)
  }
}

#[tokio::test]
async fn every_receiving_side_route_is_called_by_the_real_link_into_the_real_router() {
  use bmo_chat::ChatReceiver;

  let taker = Arc::new(Taker::default());
  let router = Router::new().nest("/receiving", bmo_chat_client::receiver_router(taker.clone(), TOKEN));
  let listening = Listening::start(router).await;
  let link = link_to(listening.base("receiving"), TOKEN);

  for ((room_id, message_id), message) in KEYS.into_iter().zip(on_provider::handed_messages()) {
    let held = link.hand_over(room_id, message_id, message.clone()).await.unwrap();
    assert_eq!(
      held,
      bmo_chat::HeldMessage {
        created: true,
        message
      }
    );
  }
  let first = on_provider::handed_messages().swap_remove(0);
  let (room_id, message_id) = KEYS[0];
  let again = bmo_chat::HandedMessage {
    origin: bmo_chat::MessageOrigin::History,
    ..first.clone()
  };
  assert_eq!(
    link.hand_over(room_id, message_id, again.clone()).await.unwrap(),
    bmo_chat::HeldMessage {
      created: false,
      message: first
    },
    "a message held before is answered 200 with the message as held"
  );
  let (room_id, message_id) = KEYS[GATHERING];
  for patch in on_provider::gathered_patches() {
    let gathered = link.gathered(room_id, message_id, patch.clone()).await.unwrap();
    let expected = bmo_chat::HandedMessage {
      files: patch.files,
      gather_failure: patch.gather_failure,
      gathering: false,
      ..on_provider::handed_messages().swap_remove(GATHERING)
    };
    assert_eq!(gathered, expected);
  }
  for change in on_provider::avatar_changes() {
    link.avatars_changed(change).await.unwrap();
  }
  let asking = on_provider::room_queries().swap_remove(0);
  for room in on_provider::receiver_rooms() {
    assert_eq!(link.room(room.id, asking.clone()).await.unwrap(), Some(room));
  }
  assert_eq!(link.room(13, asking.clone()).await.unwrap(), None, "a room not held reads as none");
  for (query, page) in on_provider::rooms_queries().into_iter().zip(on_provider::room_pages()) {
    assert_eq!(link.rooms(query).await.unwrap(), page);
  }

  let mut expected: Vec<Took> = KEYS
    .into_iter()
    .zip(on_client::handed_messages())
    .map(|((room_id, message_id), message)| Took::HandOver(room_id, message_id, Box::new(message)))
    .collect();
  let again = bmo_chat_client::HandedMessage {
    origin: bmo_chat_client::MessageOrigin::History,
    ..on_client::handed_messages().swap_remove(0)
  };
  expected.push(Took::HandOver(KEYS[0].0, KEYS[0].1, Box::new(again)));
  let (room_id, message_id) = KEYS[GATHERING];
  expected.extend(
    on_client::gathered_patches()
      .into_iter()
      .map(|patch| Took::Gathered(room_id, message_id, patch)),
  );
  expected.extend(on_client::avatar_changes().into_iter().map(Took::AvatarsChanged));
  let asked = on_client::room_queries().swap_remove(0);
  expected.extend(on_client::receiver_rooms().iter().map(|room| Took::Room(room.id, asked.clone())));
  expected.push(Took::Room(13, asked));
  expected.extend(on_client::rooms_queries().into_iter().map(|query| {
    Took::Rooms(bmo_chat_client::RoomsQuery {
      limit: query.limit.map(|limit| limit + 1),
      ..query
    })
  }));
  assert_eq!(
    *taker.took.lock().unwrap(),
    expected,
    "the router asks for one room more than the limit"
  );

  let given = on_client::wire_errors();
  for (index, error) in on_provider::wire_errors().into_iter().enumerate() {
    if !matches!(error.code, bmo_chat::ErrorCode::NotReady | bmo_chat::ErrorCode::Failed) {
      continue;
    }
    *taker.refusing.lock().unwrap() = Some(given[index].clone());
    let (room_id, message_id) = KEYS[0];
    let failure = link.hand_over(room_id, message_id, on_provider::handed_messages().swap_remove(0)).await;
    assert_eq!(link_answered_with(failure.unwrap_err()), (error.code.status(), Some(error)));
  }
  assert_eq!(taker.took.lock().unwrap().len(), expected.len(), "a refused call is not taken");

  let stranger = link_to(listening.base("receiving"), "a-wrong-token");
  let failure = stranger
    .avatars_changed(on_provider::avatar_changes().swap_remove(0))
    .await
    .unwrap_err();
  let (status, answer) = link_answered_with(failure);
  assert_eq!(
    (status, answer.map(|answer| answer.code)),
    (401, Some(bmo_chat::ErrorCode::Unauthorized))
  );

  drop((link, stranger));
  listening.close().await;
}
