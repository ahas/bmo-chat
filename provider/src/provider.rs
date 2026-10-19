//! The provider contract: what a chat service implements to be a provider, and the receiver as a provider
//! calls it.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::avatar::{ChatAvatar, ChatAvatarKind, ChatPicture};
use crate::bodies::ChatHealth;
use crate::capability::ChatCapability;
use crate::declaration::ChatDeclaration;
use crate::dialect::ChatDialect;
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::message::{ChatGathered, ChatHandover, RoomKind};
use crate::modes::{ChatModeStanding, ChatReadMode, ChatVoice};
use crate::numbering::ChatNumbering;
use crate::receiver_bodies::{AvatarsUpdated, HistoryRoom, ListenedQuery, RoomsOfQuery};
use crate::refusal::ChatRefusal;
use crate::send::{ChatSendRequest, ChatSendResult};

/// A call that could not be answered at all. Its words are the English sentence the error shape carries.
pub type ChatFailure = Box<dyn std::error::Error + Send + Sync>;

/// The receiver's side as a provider calls it. A provider asks nothing else of the receiver, and hands
/// nothing over as already archived.
#[async_trait]
pub trait ChatReceiver: Send + Sync {
  /// A message a room said.
  async fn message(&self, handover: ChatHandover) -> Result<(), ChatFailure>;

  /// A message carrying a file, with the file or the promise of it.
  async fn attachment(&self, handover: ChatHandover) -> Result<(), ChatFailure>;

  /// A message already handed over, changed: the receiver keeps its words and its sender as handed over.
  async fn edited(&self, handover: ChatHandover) -> Result<(), ChatFailure>;

  /// The file a message's `gathering` promised, or why it was not kept.
  async fn gathered(&self, gathered: ChatGathered) -> Result<(), ChatFailure>;

  /// A past message read back from a room, archived where the receiver lacks it and waking nobody. True
  /// where the receiver archived it, false where it already held it.
  async fn history(&self, handover: ChatHandover) -> Result<bool, ChatFailure>;

  /// Word that the kept profile pictures of these rooms and people changed.
  async fn avatars_updated(&self, updated: AvatarsUpdated) -> Result<(), ChatFailure>;

  /// Whether the receiver listens to the room.
  async fn listened(&self, query: ListenedQuery) -> Result<bool, ChatFailure>;

  /// The provider's rooms whose past it may read back, each with the oldest message the receiver holds of
  /// it, in ascending order of room number.
  async fn rooms_of(&self, query: RoomsOfQuery) -> Result<Vec<HistoryRoom>, ChatFailure>;
}

/// A chat service's one platform, as the base serves it. Every difference between providers is what the
/// provider declares and answers here; the base never asks which provider it is.
#[async_trait]
pub trait ChatProvider: Send + Sync {
  /// Short, ASCII and stable; written onto every message the provider hands over.
  fn key(&self) -> &str;

  /// What the platform is called where a person reads it.
  fn label(&self) -> &str;

  /// Null when the provider can run, else the reason it cannot.
  fn configured(&self) -> Option<String>;

  /// Opens the platform connection for each active mode and for no other. The receiver link is all the
  /// provider is given; it reads its own configuration from its own environment.
  async fn start(&self, receiver: Arc<dyn ChatReceiver>) -> Result<(), ChatFailure>;

  async fn stop(&self);

  /// Whether the provider is ready to answer about rooms and to send. One that says nothing of its own is
  /// ready exactly when it is configured.
  fn health(&self) -> ChatHealth {
    match self.configured() {
      None => ChatHealth::ready(),
      Some(reason) => ChatHealth::not_ready(reason),
    }
  }

  /// Every mode the provider supports, each with where it stands now.
  fn modes(&self) -> Vec<ChatReadMode>;

  fn can(&self) -> BTreeSet<ChatCapability>;

  fn dialect(&self) -> &dyn ChatDialect;

  /// What a person reading this provider's rooms sees beyond what the provider sends; null where it shows
  /// only what it carries.
  fn shows(&self) -> Option<String> {
    None
  }

  fn numbering(&self) -> ChatNumbering;

  /// Whether a send may name an agent speaker other than the one the room is answered by.
  fn speakers(&self) -> bool {
    false
  }

  /// Whether the room is one of this provider's.
  async fn owns_room(&self, chat_id: i64) -> Result<bool, ChatFailure>;

  /// The sender number the agent's own words carry in the room, or null.
  async fn identity_of(&self, chat_id: i64) -> Result<Option<i64>, ChatFailure>;

  /// `owner` when the room is the owner's private chat on this platform, else `group`.
  async fn is_owner_room(&self, chat_id: i64) -> Result<RoomKind, ChatFailure>;

  /// The sender number of the one person in a one-to-one with the agent, or null.
  async fn private_room_of(&self, chat_id: i64) -> Result<Option<i64>, ChatFailure>;

  /// The room to speak into to reach the owner unprompted, or null.
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure>;

  /// What the platform calls the room, or null where it cannot or will not say.
  async fn room_title(&self, chat_id: i64) -> Result<Option<String>, ChatFailure>;

  /// One send. A send the provider does not perform is answered as refused; a failure while performing it
  /// is the error.
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure>;

  /// The picture or the file a message carried, fetched from the platform again. A provider that supplies
  /// no fetch of its own refuses with `unsupported-capability`.
  async fn fetch_media(&self, _request: ChatMediaRequest) -> Result<ChatMediaResult, ChatFailure> {
    Ok(ChatMediaResult::Refused {
      refusal: self.unsupported(ChatCapability::FetchMedia),
    })
  }

  /// Every profile picture the provider keeps. The base pages the list and asks only a provider that declares
  /// `avatars`; one that supplies no list of its own keeps none.
  async fn avatars(&self) -> Result<Vec<ChatAvatar>, ChatFailure> {
    Ok(Vec::new())
  }

  /// The profile picture the provider keeps for a room or a person, or null where it keeps none. The base asks
  /// only a provider that declares `avatars`.
  async fn avatar(&self, _kind: ChatAvatarKind, _id: i64) -> Result<Option<ChatPicture>, ChatFailure> {
    Ok(None)
  }

  /// The provider's mode for a voice, or null where it has none.
  fn mode_of(&self, voice: ChatVoice) -> Option<ChatReadMode> {
    self.modes().into_iter().find(|mode| mode.voice == voice)
  }

  /// The modes that are started; a mode that is not active never is.
  fn active_modes(&self) -> Vec<ChatReadMode> {
    let mut modes = self.modes();
    modes.retain(|mode| mode.standing == ChatModeStanding::Active);
    modes
  }

  /// The refusal of something the provider is asked to do, naming the provider and what was asked.
  fn unsupported(&self, capability: ChatCapability) -> ChatRefusal {
    ChatRefusal::UnsupportedCapability {
      label: self.label().to_owned(),
      capability,
    }
  }

  /// That refusal where the provider does not declare the capability; null where it does.
  fn undeclared(&self, capability: ChatCapability) -> Option<ChatRefusal> {
    (!self.can().contains(&capability)).then(|| self.unsupported(capability))
  }

  /// The provider's self-declaration, gathered from its members as they stand now.
  fn declaration(&self) -> ChatDeclaration {
    ChatDeclaration {
      key: self.key().to_owned(),
      label: self.label().to_owned(),
      configured: self.configured(),
      modes: self.modes(),
      can: self.can(),
      dialect: self.dialect().declaration(),
      shows: self.shows(),
      numbering: self.numbering(),
      speakers: self.speakers(),
    }
  }
}
