//! The chat provider side of the chat protocol.
//!
//! A chat service imports this crate to be a provider: the chat piece types, the provider contract, the
//! server for the provider routes and the link that hands what a room said to the receiver. The pieces are
//! stated in `docs/contract.md` and the routes in `docs/protocol.md`.

mod avatar;
mod bodies;
mod bytes;
mod capability;
mod declaration;
mod dialect;
mod error;
mod link;
mod media;
mod message;
mod modes;
mod numbering;
mod plain;
mod provider;
mod receiver_bodies;
mod refusal;
mod send;
mod server;

pub use avatar::{AvatarList, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
pub use bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
pub use capability::{ChatCapability, ChatVerb};
pub use declaration::ChatDeclaration;
pub use dialect::{ChatDialect, ChatDialectDeclaration, ChatDialectLimits, ChatMarkup};
pub use error::{ErrorCode, WireError};
pub use link::{ChatPatience, ChatReceiverLink, ChatWait, LinkFailure, RealClock};
pub use media::{ChatMediaRequest, ChatMediaResult};
pub use message::{
  ChatFileKind, ChatGathered, ChatHandover, ChatMessage, FileItem, GatheredOutcome, InboundChatContext,
  MessageFile, RoomFacts, RoomKind,
};
pub use modes::{ChatModeStanding, ChatReach, ChatReadMode, ChatVoice};
pub use numbering::{ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering};
pub use plain::PlainDialect;
pub use provider::{ChatFailure, ChatProvider, ChatReceiver};
pub use receiver_bodies::{
  AvatarsUpdated, HistoryKept, HistoryRoom, Listened, ListenedQuery, RoomsOf, RoomsOfQuery,
};
pub use refusal::{ChatRefusal, MissedPicture, MissedPictureReason};
pub use send::{AlbumItem, ChatSend, ChatSendRequest, ChatSendResult, ChatSpeaker};
pub use server::provider_router;
