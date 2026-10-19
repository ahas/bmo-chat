//! The provider contract, held against a stand-in provider written for these tests alone: everything the
//! contract asks for and nothing a platform would bring.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bmo_chat::{
  AvatarsUpdated, ChatCapability, ChatDialect, ChatDialectLimits, ChatFailure, ChatGathered, ChatHandover,
  ChatMarkup, ChatMediaRequest, ChatMediaResult, ChatMessage, ChatModeStanding, ChatNumberRange,
  ChatNumberRanges, ChatNumberSpace, ChatNumbering, ChatProvider, ChatReach, ChatReadMode, ChatReceiver,
  ChatRefusal, ChatSend, ChatSendRequest, ChatSendResult, ChatVerb, ChatVoice, HistoryRoom, ListenedQuery,
  PlainDialect, RoomKind, RoomsOfQuery,
};

const ROOM: i64 = -100;
const OWNER: i64 = 42;

/// A dialect that folds by saying so, and converts by marking what it read.
struct Folding;

impl ChatDialect for Folding {
  fn name(&self) -> &str {
    "folding-markup"
  }
  fn limits(&self) -> ChatDialectLimits {
    ChatDialectLimits {
      text: Some(100),
      ..ChatDialectLimits::default()
    }
  }
  fn writing(&self) -> &str {
    "What a room of this stand-in dialect is told its marks become."
  }
  fn foldable(&self) -> bool {
    true
  }
  fn convert(&self, markdown: &str) -> ChatMarkup {
    ChatMarkup {
      markup: format!("<converted>{markdown}</converted>"),
      plain: markdown.to_owned(),
      formatted: true,
    }
  }
  fn parts(&self, markdown: &str) -> Vec<String> {
    vec![markdown.to_owned()]
  }
  fn fault(&self, _markup: &str) -> Option<String> {
    None
  }
  /// Nothing short is worth folding.
  fn fold(&self, markdown: &str) -> Option<String> {
    (markdown.len() > 10).then(|| format!("<folded>{markdown}</folded>"))
  }
  fn visible(&self, markup: &str) -> String {
    markup.to_owned()
  }
}

/// A dialect that cannot fold, and fails the test if it is ever asked to.
struct Flat;

impl ChatDialect for Flat {
  fn name(&self) -> &str {
    "flat-words"
  }
  fn limits(&self) -> ChatDialectLimits {
    ChatDialectLimits::default()
  }
  fn writing(&self) -> &str {
    "What a room of this stand-in dialect is told its marks become."
  }
  fn foldable(&self) -> bool {
    false
  }
  fn convert(&self, markdown: &str) -> ChatMarkup {
    ChatMarkup {
      markup: markdown.to_owned(),
      plain: markdown.to_owned(),
      formatted: false,
    }
  }
  fn parts(&self, markdown: &str) -> Vec<String> {
    vec![markdown.to_owned()]
  }
  fn fault(&self, _markup: &str) -> Option<String> {
    None
  }
  fn fold(&self, _markdown: &str) -> Option<String> {
    panic!("a dialect that cannot fold was asked for a fold")
  }
  fn visible(&self, markup: &str) -> String {
    markup.to_owned()
  }
}

fn mode(voice: ChatVoice, standing: ChatModeStanding) -> ChatReadMode {
  ChatReadMode {
    voice,
    reach: ChatReach::AllMessages,
    standing,
    reason: (standing != ChatModeStanding::Active).then(|| "Nothing is configured for it.".to_owned()),
    can_reply_to: voice == ChatVoice::BotAccount,
  }
}

/// The stand-in provider. It keeps what it was asked to do so a test can read it back.
struct StandIn {
  key: &'static str,
  label: &'static str,
  can: BTreeSet<ChatCapability>,
  dialect: Box<dyn ChatDialect>,
  receiver: Mutex<Option<Arc<dyn ChatReceiver>>>,
  started: Mutex<Vec<ChatVoice>>,
  sent: Mutex<Vec<String>>,
}

impl StandIn {
  fn new(can: &[ChatCapability], dialect: Box<dyn ChatDialect>) -> Self {
    Self {
      key: "stand-in",
      label: "Platform",
      can: can.iter().copied().collect(),
      dialect,
      receiver: Mutex::new(None),
      started: Mutex::new(Vec::new()),
      sent: Mutex::new(Vec::new()),
    }
  }

  fn sent(&self) -> Vec<String> {
    self.sent.lock().unwrap().clone()
  }

  /// A room speaking: the stand-in hands what was said over the link it was started with.
  async fn hears(&self, text: &str) {
    let receiver = self.receiver.lock().unwrap().clone().expect("the provider was started");
    let message = ChatMessage {
      chat_id: ROOM,
      message_id: 5,
      sender_id: Some(OWNER),
      sender_name: "A Person".into(),
      sent_at: 1_760_000_000_000,
      text: text.into(),
      reply_to_message_id: None,
      via: self.key.into(),
      album: None,
      mode: ChatVoice::BotAccount,
    };
    receiver
      .message(ChatHandover {
        message,
        context: None,
        room: None,
        files: vec![],
      })
      .await
      .expect("the receiver took it");
  }
}

#[async_trait]
impl ChatProvider for StandIn {
  fn key(&self) -> &str {
    self.key
  }
  fn label(&self) -> &str {
    self.label
  }
  fn configured(&self) -> Option<String> {
    None
  }
  async fn start(&self, receiver: Arc<dyn ChatReceiver>) -> Result<(), ChatFailure> {
    *self.receiver.lock().unwrap() = Some(receiver);
    *self.started.lock().unwrap() = self.active_modes().into_iter().map(|mode| mode.voice).collect();
    Ok(())
  }
  async fn stop(&self) {
    self.receiver.lock().unwrap().take();
    self.started.lock().unwrap().clear();
  }
  fn modes(&self) -> Vec<ChatReadMode> {
    vec![
      mode(ChatVoice::BotAccount, ChatModeStanding::Active),
      mode(ChatVoice::HumanReader, ChatModeStanding::Inactive),
      mode(ChatVoice::HumanAvatar, ChatModeStanding::Unsupported),
    ]
  }
  fn can(&self) -> BTreeSet<ChatCapability> {
    self.can.clone()
  }
  fn dialect(&self) -> &dyn ChatDialect {
    self.dialect.as_ref()
  }
  fn numbering(&self) -> ChatNumbering {
    ChatNumbering {
      rooms: ChatNumberSpace::Assigned,
      people: ChatNumberSpace::Assigned,
      messages: ChatNumberSpace::Assigned,
      ranges: ChatNumberRanges {
        rooms: Some(ChatNumberRange {
          first: -2_000_000,
          last: -1_000_001,
        }),
        people: Some(ChatNumberRange {
          first: 1_000_001,
          last: 2_000_000,
        }),
      },
    }
  }
  async fn owns_room(&self, chat_id: i64) -> Result<bool, ChatFailure> {
    Ok(chat_id == ROOM || chat_id == OWNER)
  }
  async fn identity_of(&self, _chat_id: i64) -> Result<Option<i64>, ChatFailure> {
    Ok(Some(7))
  }
  async fn is_owner_room(&self, chat_id: i64) -> Result<RoomKind, ChatFailure> {
    Ok(if chat_id == OWNER { RoomKind::Owner } else { RoomKind::Group })
  }
  async fn private_room_of(&self, chat_id: i64) -> Result<Option<i64>, ChatFailure> {
    Ok((chat_id == OWNER).then_some(OWNER))
  }
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure> {
    Ok(Some(OWNER))
  }
  async fn room_title(&self, chat_id: i64) -> Result<Option<String>, ChatFailure> {
    Ok((chat_id == ROOM).then(|| "A Room".to_owned()))
  }
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure> {
    if let Some(refusal) = self.undeclared(request.send.verb().into()) {
      return Ok(ChatSendResult::Refused {
        refusal,
        gone: false,
      });
    }
    if let ChatSend::Text { text, fold, .. } = &request.send {
      self.sent.lock().unwrap().push(self.dialect.outgoing(text, *fold));
    }
    Ok(ChatSendResult::Sent {
      message_id: Some(1),
      missed: vec![],
    })
  }
}

/// The receiver as the stand-in reaches it: it keeps what it was handed.
#[derive(Default)]
struct Receiver {
  handed: Mutex<Vec<String>>,
}

#[async_trait]
impl ChatReceiver for Receiver {
  async fn message(&self, handover: ChatHandover) -> Result<(), ChatFailure> {
    self.handed.lock().unwrap().push(handover.message.text);
    Ok(())
  }
  async fn attachment(&self, _handover: ChatHandover) -> Result<(), ChatFailure> {
    Err("the stand-in receiver takes no attachment".into())
  }
  async fn edited(&self, _handover: ChatHandover) -> Result<(), ChatFailure> {
    Err("the stand-in receiver takes no edit".into())
  }
  async fn gathered(&self, _gathered: ChatGathered) -> Result<(), ChatFailure> {
    Err("the stand-in receiver takes no gathered file".into())
  }
  async fn history(&self, _handover: ChatHandover) -> Result<bool, ChatFailure> {
    Err("the stand-in receiver takes no past message".into())
  }
  async fn avatars_updated(&self, _updated: AvatarsUpdated) -> Result<(), ChatFailure> {
    Err("the stand-in receiver takes no word of pictures".into())
  }
  async fn listened(&self, _query: ListenedQuery) -> Result<bool, ChatFailure> {
    Ok(true)
  }
  async fn rooms_of(&self, _query: RoomsOfQuery) -> Result<Vec<HistoryRoom>, ChatFailure> {
    Ok(Vec::new())
  }
}

fn text(text: &str, fold: bool) -> ChatSendRequest {
  ChatSendRequest::new(
    ROOM,
    ChatSend::Text {
      text: text.into(),
      reply_to: None,
      silent: false,
      fold,
    },
  )
}

#[tokio::test]
async fn the_stand_in_answers_the_whole_contract_as_a_shared_provider() {
  let stand_in = Arc::new(StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect)));
  let provider: Arc<dyn ChatProvider> = stand_in.clone();

  assert_eq!(provider.key(), "stand-in");
  assert_eq!(provider.label(), "Platform");
  assert_eq!(provider.configured(), None);
  assert_eq!(provider.shows(), None);
  assert!(!provider.speakers());
  assert!(provider.owns_room(ROOM).await.unwrap());
  assert!(!provider.owns_room(-777).await.unwrap());
  assert_eq!(provider.identity_of(ROOM).await.unwrap(), Some(7));
  assert_eq!(provider.is_owner_room(OWNER).await.unwrap(), RoomKind::Owner);
  assert_eq!(provider.is_owner_room(ROOM).await.unwrap(), RoomKind::Group);
  assert_eq!(provider.private_room_of(OWNER).await.unwrap(), Some(OWNER));
  assert_eq!(provider.private_room_of(ROOM).await.unwrap(), None);
  assert_eq!(provider.owner_room().await.unwrap(), Some(OWNER));
  assert_eq!(provider.room_title(ROOM).await.unwrap(), Some("A Room".into()));
  assert_eq!(provider.room_title(OWNER).await.unwrap(), None);
  assert_eq!(
    provider.send(text("hello", false)).await.unwrap(),
    ChatSendResult::Sent {
      message_id: Some(1),
      missed: vec![]
    }
  );
}

#[tokio::test]
async fn the_declaration_is_gathered_from_the_members_and_crosses_the_wire() {
  let provider = StandIn::new(&[ChatCapability::Text, ChatCapability::ReplyTo], Box::new(PlainDialect));
  let declaration = provider.declaration();

  assert_eq!(declaration.key, provider.key());
  assert_eq!(declaration.label, provider.label());
  assert_eq!(declaration.modes, provider.modes());
  assert_eq!(declaration.can, provider.can());
  assert_eq!(declaration.dialect, provider.dialect().declaration());
  assert_eq!(declaration.numbering, provider.numbering());
  let wire = serde_json::to_value(&declaration).unwrap();
  assert_eq!(wire["can"], serde_json::json!(["text", "replyTo"]));
  assert_eq!(wire["dialect"]["name"], "plain");
  assert_eq!(wire["dialect"]["foldable"], false);
  assert_eq!(serde_json::from_value::<bmo_chat::ChatDeclaration>(wire).unwrap(), declaration);
}

#[tokio::test]
async fn start_is_given_only_the_receiver_link_and_starts_only_the_active_modes() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect));
  let receiver = Arc::new(Receiver::default());

  provider.start(receiver.clone()).await.unwrap();
  assert_eq!(*provider.started.lock().unwrap(), [ChatVoice::BotAccount]);
  provider.hears("hello").await;
  assert_eq!(*receiver.handed.lock().unwrap(), ["hello"]);

  provider.stop().await;
  assert!(provider.started.lock().unwrap().is_empty());
  assert!(provider.receiver.lock().unwrap().is_none());
}

#[test]
fn the_mode_a_message_came_in_on_is_found_by_its_voice() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect));

  assert_eq!(provider.mode_of(ChatVoice::BotAccount).map(|mode| mode.can_reply_to), Some(true));
  assert_eq!(
    provider.mode_of(ChatVoice::HumanReader).map(|mode| mode.standing),
    Some(ChatModeStanding::Inactive)
  );
  assert_eq!(provider.mode_of(ChatVoice::Direct), None);
  assert_eq!(provider.active_modes(), [mode(ChatVoice::BotAccount, ChatModeStanding::Active)]);
}

#[test]
fn an_undeclared_verb_is_refused_with_one_sentence_naming_what_was_asked_and_the_platform() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect));
  let refusal = provider.undeclared(ChatCapability::Sticker).expect("a sticker is not declared");

  assert_eq!(
    refusal,
    ChatRefusal::UnsupportedCapability {
      label: "Platform".into(),
      capability: ChatCapability::Sticker
    }
  );
  assert_eq!(refusal.code(), "unsupported-capability");
  assert_eq!(refusal.to_string(), "Platform cannot send a sticker.");
  assert_eq!(provider.undeclared(ChatCapability::Text), None);
}

#[tokio::test]
async fn and_the_refusal_is_what_the_send_answers_rather_than_a_failure_or_a_silence() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect));
  let sticker = ChatSendRequest::new(
    ROOM,
    ChatSend::Sticker {
      emoji: "🙂".into()
    },
  );

  assert_eq!(
    provider.send(sticker).await.unwrap(),
    ChatSendResult::Refused {
      refusal: provider.unsupported(ChatCapability::Sticker),
      gone: false
    }
  );
  assert!(matches!(provider.send(text("here", false)).await.unwrap(), ChatSendResult::Sent { .. }));
}

#[tokio::test]
async fn every_verb_a_provider_does_not_declare_is_refused_by_the_one_mechanism() {
  let provider = StandIn::new(&[], Box::new(PlainDialect));

  for verb in ChatVerb::ALL {
    let refusal = provider.undeclared((*verb).into()).expect("nothing is declared");
    assert_eq!(refusal.to_string(), format!("Platform cannot {}.", ChatCapability::from(*verb).words()));
  }
  let result = provider.send(text("hello", false)).await.unwrap();
  assert!(
    matches!(result, ChatSendResult::Refused { refusal, .. } if refusal.code() == "unsupported-capability")
  );
}

#[tokio::test]
async fn a_provider_with_no_fetch_of_its_own_refuses_one() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(PlainDialect));
  let request = ChatMediaRequest {
    chat_id: ROOM,
    message_id: 5,
    sent_at: None,
  };

  assert_eq!(
    provider.fetch_media(request).await.unwrap(),
    ChatMediaResult::Refused {
      refusal: ChatRefusal::UnsupportedCapability {
        label: "Platform".into(),
        capability: ChatCapability::FetchMedia
      }
    }
  );
}

#[tokio::test]
async fn a_foldable_dialect_folds_a_text_that_asks_for_it() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(Folding));

  provider.send(text("a long answer", true)).await.unwrap();
  provider.send(text("a long answer", false)).await.unwrap();
  // Nothing worth folding goes out as the conversion.
  provider.send(text("short", true)).await.unwrap();

  assert_eq!(
    provider.sent(),
    [
      "<folded>a long answer</folded>",
      "<converted>a long answer</converted>",
      "<converted>short</converted>",
    ]
  );
  assert!(provider.declaration().dialect.foldable);
  assert_eq!(Folding.folded("a long answer").as_deref(), Some("<folded>a long answer</folded>"));
}

#[tokio::test]
async fn a_dialect_that_cannot_fold_is_never_asked_for_a_fold() {
  let provider = StandIn::new(&[ChatCapability::Text], Box::new(Flat));

  provider.send(text("a long answer", true)).await.unwrap();

  assert_eq!(provider.sent(), ["a long answer"]);
  assert!(!provider.declaration().dialect.foldable);
  assert_eq!(Flat.folded("a long answer"), None);
}

#[test]
fn the_full_capability_set_is_all_sixteen_from_the_type_itself() {
  let names: Vec<String> = ChatCapability::ALL
    .iter()
    .map(|capability| serde_json::to_value(capability).unwrap().as_str().unwrap().to_owned())
    .collect();
  assert_eq!(
    names,
    [
      "text",
      "photo",
      "album",
      "document",
      "audio",
      "voice",
      "sticker",
      "reaction",
      "pin",
      "typing",
      "edit",
      "delete",
      "replyTo",
      "fetchMedia",
      "avatars",
      "history",
    ]
  );
  assert_eq!(ChatCapability::ALL.iter().collect::<BTreeSet<_>>().len(), 16);
  assert_eq!(ChatVerb::ALL.len(), 12);
  for verb in ChatVerb::ALL {
    assert!(ChatCapability::ALL.contains(&ChatCapability::from(*verb)));
  }
}
