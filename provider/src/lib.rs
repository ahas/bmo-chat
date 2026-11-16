//! The chat provider side of the chat protocol.
//!
//! A chat service imports this crate to be a provider: the chat piece types, the provider contract, the
//! server for the provider routes and the link that hands what a room said to the receiving side. The pieces are
//! stated in `docs/contract.md` and the routes in `docs/protocol.md`.

mod avatar;
mod bodies;
mod capability;
mod contacts;
mod declaration;
mod dialect;
mod error;
mod file_body;
mod link;
mod media;
mod members;
mod message;
mod modes;
mod numbering;
mod parts;
mod plain;
mod presence;
mod provider;
mod receiver_bodies;
mod refusal;
mod send;
mod server;

pub use avatar::{AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
pub use bodies::{
  AccountRoomsQuery, ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, Page,
  ProviderOwner, ProviderRoom,
};
pub use capability::{ChatCapability, ChatVerb};
pub use contacts::{
  ChatContact, ChatContactKind, ChatStart, ChatStartRequest, ChatStartResult, ContactsQuery,
};
pub use declaration::ChatDeclaration;
pub use dialect::{ChatDialect, ChatDialectDeclaration, ChatDialectLimits, ChatMarkup};
pub use error::{ErrorCode, RETRY_AFTER_SECONDS, WireError};
pub use file_body::{FileBody, StoredFile};
pub use link::{ChatPatience, ChatReceiverLink, ChatWait, LinkFailure, RealClock};
pub use media::{ChatMediaRequest, ChatMediaResult, FilesQuery};
pub use members::{ChatMember, ChatMembers, ChatMembersResult};
pub use message::{
  ChatFileKind, FileItem, GatheredPatch, HandedMessage, InboundChatContext, MessageFile, MessageOrigin,
  RoomFacts, RoomKind,
};
pub use modes::{ChatModeStanding, ChatReach, ChatReadMode, ChatVoice};
pub use numbering::{ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering};
pub use plain::PlainDialect;
pub use presence::{
  ChatPresence, ChatPresenceState, PersonPresence, PresenceChange, TYPING_MARK_LASTS_MS, TypingMark,
};
pub use provider::{ChatFailure, ChatProvider, ChatReceiver};
pub use receiver_bodies::{
  AvatarChange, Deletion, HeldMessage, MessageStamp, PersonReactions, ReactionChange, ReceiverRoom,
  RoomQuery, RoomsQuery,
};
pub use refusal::{ChatRefusal, MissedPicture, MissedPictureReason};
pub use send::{AlbumItem, ChatSend, ChatSendRequest, ChatSendResult, ChatSent, ChatSticker, StickerItem};
pub use server::provider_router;
