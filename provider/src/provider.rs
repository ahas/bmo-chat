//! The provider contract: what a chat service implements to be a provider, and the receiving side as a
//! provider calls it.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use crate::avatar::{ChatAvatar, ChatAvatarKind, ChatPicture};
use crate::bodies::{ChatHealth, Page, ProviderRoom};
use crate::capability::ChatCapability;
use crate::contacts::{ChatContact, ChatStartRequest, ChatStartResult};
use crate::declaration::ChatDeclaration;
use crate::dialect::ChatDialect;
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::members::ChatMembersResult;
use crate::message::{GatheredPatch, HandedMessage};
use crate::modes::{ChatModeStanding, ChatReadMode, ChatVoice};
use crate::numbering::ChatNumbering;
use crate::presence::{PresenceChange, TypingMark};
use crate::receiver_bodies::{
  AvatarChange, Deletion, HeldMessage, ReactionChange, ReceiverRoom, RoomQuery, RoomsQuery,
};
use crate::refusal::ChatRefusal;
use crate::send::{ChatSendRequest, ChatSendResult};

/// A call that could not be answered at all. Its words are the English sentence the error shape carries.
pub type ChatFailure = Box<dyn std::error::Error + Send + Sync>;

/// The receiving side as a provider calls it. A provider asks nothing else of it, and hands nothing over as
/// already kept.
#[async_trait]
pub trait ChatReceiver: Send + Sync {
  /// A message of a room handed over, whatever its origin: the message as the receiving side now holds it,
  /// and whether it held it before.
  async fn hand_over(
    &self,
    room_id: i64,
    message_id: i64,
    message: HandedMessage,
  ) -> Result<HeldMessage, ChatFailure>;

  /// The files a message's gathering promised, or why they were not kept: the message as the receiving side
  /// now holds it.
  async fn gathered(
    &self,
    room_id: i64,
    message_id: i64,
    patch: GatheredPatch,
  ) -> Result<HandedMessage, ChatFailure>;

  /// Word that the kept profile pictures of these rooms and people changed.
  async fn avatars_changed(&self, change: AvatarChange) -> Result<(), ChatFailure>;

  /// One message's reactions as the platform shows them now.
  async fn reactions_changed(&self, change: ReactionChange) -> Result<(), ChatFailure>;

  /// Messages of one room deleted on the platform.
  async fn deleted(&self, deletion: Deletion) -> Result<(), ChatFailure>;

  /// A person writing in a room now, or no longer. A receiving side that shows no typing passes it over.
  async fn typing_marked(&self, _mark: TypingMark) -> Result<(), ChatFailure> {
    Ok(())
  }

  /// Whether people are reachable now. A receiving side that shows no presence passes it over.
  async fn presence_changed(&self, _change: PresenceChange) -> Result<(), ChatFailure> {
    Ok(())
  }

  /// The room as the receiving side holds it, or null where it holds nothing of it, which reads as a room
  /// not listened to.
  async fn room(&self, room_id: i64, query: RoomQuery) -> Result<Option<ReceiverRoom>, ChatFailure>;

  /// A page of the provider's rooms whose past it may read back, in ascending order of room number.
  async fn rooms(&self, query: RoomsQuery) -> Result<Page<ReceiverRoom>, ChatFailure>;
}

/// A chat service's one platform, as the base serves it. Every difference between providers is what the
/// provider declares and answers here; the base never asks which provider it is.
#[async_trait]
pub trait ChatProvider: Send + Sync {
  /// Short, ASCII and stable; written onto every message the provider hands over.
  fn key(&self) -> &str;

  /// What the platform is called where a person reads it.
  fn label(&self) -> &str;

  /// The provider service's own version, in semver, as its health answers it.
  fn version(&self) -> &str;

  /// Null when the provider can run, else the reason it cannot.
  fn configured(&self) -> Option<String>;

  /// Opens the platform connection for each active mode and for no other. The link to the receiving side is
  /// all the provider is given; it reads its own configuration from its own environment.
  async fn start(&self, receiver: Arc<dyn ChatReceiver>) -> Result<(), ChatFailure>;

  async fn stop(&self);

  /// Whether the provider is ready to answer about rooms and to send. One that says nothing of its own is
  /// ready exactly when it is configured.
  fn health(&self) -> ChatHealth {
    match self.configured() {
      None => ChatHealth::ready(self.version()),
      Some(reason) => ChatHealth::not_ready(self.version(), reason),
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

  /// Whether a send may name a speaker other than the one the room is answered by.
  fn speakers(&self) -> bool {
    false
  }

  /// The directory each file a multipart send carries is written into as it arrives (`contract.md`, "A
  /// file's bytes"): the system's temporary directory unless the provider names its own.
  fn spool(&self) -> PathBuf {
    std::env::temp_dir()
  }

  /// What the provider knows of the room, or null where it knows nothing of it.
  async fn room(&self, room_id: i64) -> Result<Option<ProviderRoom>, ChatFailure>;

  /// The room to speak into to reach the owner unprompted, or null.
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure>;

  /// Every room the provider's account is in, each as the provider answers it on its own. The base pages the
  /// list and asks only a provider that declares `roomList`; one that supplies no list of its own lists none.
  async fn account_rooms(&self) -> Result<Vec<ProviderRoom>, ChatFailure> {
    Ok(Vec::new())
  }

  /// Every contact of the account named, or of the provider's own where none is (`contract.md`, "The account a
  /// call is for"). The base pages the list and asks only a provider that declares `contactList`; one that
  /// supplies no list of its own lists none.
  async fn contacts(&self, _account_id: Option<i64>) -> Result<Vec<ChatContact>, ChatFailure> {
    Ok(Vec::new())
  }

  /// The members of a room (`contract.md`, "The members of a room"). The base asks only a provider that declares
  /// `memberList`; one that supplies no list of its own refuses with `unsupported-capability`.
  async fn members(&self, _room_id: i64) -> Result<ChatMembersResult, ChatFailure> {
    Ok(ChatMembersResult::Refused(self.unsupported(ChatCapability::MemberList)))
  }

  /// A conversation started on the platform, in the account the request names or in the provider's own. The
  /// base asks only a provider that declares the start's capability, with a start of a sound shape; one that
  /// supplies no start of its own refuses with `unsupported-capability`.
  async fn start_chat(&self, request: ChatStartRequest) -> Result<ChatStartResult, ChatFailure> {
    Ok(ChatStartResult::Refused(self.unsupported(request.start.capability())))
  }

  /// One send. A send the provider does not perform is answered as refused; a failure while performing it
  /// is the error.
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure>;

  /// The pictures and files a message carried, fetched from the platform again. A provider that supplies no
  /// fetch of its own refuses with `unsupported-capability`.
  async fn fetch_media(&self, _request: ChatMediaRequest) -> Result<ChatMediaResult, ChatFailure> {
    Ok(ChatMediaResult::Refused(self.unsupported(ChatCapability::FetchMedia)))
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
