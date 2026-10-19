//! The receiver side of the chat protocol.
//!
//! The receiver imports this crate to reach every chat provider through one client and to serve the routes
//! a provider hands messages to. The pieces are stated in `docs/contract.md` and the routes in
//! `docs/protocol.md`. This crate defines every wire shape on its own; a test at the repository root checks
//! that they agree with the provider side's.

mod avatar;
mod bodies;
mod bytes;
mod capability;
mod declaration;
mod dialect;
mod error;
mod inbound;
mod media;
mod message;
mod modes;
mod numbering;
mod openapi;
mod patience;
mod provider_client;
mod receiver_bodies;
mod refusal;
mod send;

pub use avatar::{AvatarList, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
pub use bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
pub use capability::{ChatCapability, ChatVerb};
pub use declaration::ChatDeclaration;
pub use dialect::{ChatDialectDeclaration, ChatDialectLimits, ChatMarkup};
pub use error::{ErrorCode, WireError};
pub use inbound::{ChatReceiver, receiver_router};
pub use media::{ChatMediaRequest, ChatMediaResult};
pub use message::{
  ChatFileKind, ChatGathered, ChatHandover, ChatMessage, FileItem, GatheredOutcome, InboundChatContext,
  MessageFile, RoomFacts, RoomKind,
};
pub use modes::{ChatModeStanding, ChatReach, ChatReadMode, ChatVoice};
pub use numbering::{ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering};
pub use openapi::{openapi, openapi_json};
pub use patience::{ChatPatience, ChatWait, RealClock};
pub use provider_client::{ChatProviderClient, ClientFailure};
pub use receiver_bodies::{
  AvatarsUpdated, HistoryKept, HistoryRoom, Listened, ListenedQuery, RoomsOf, RoomsOfQuery,
};
pub use refusal::{ChatRefusal, MissedPicture, MissedPictureReason};
pub use send::{AlbumItem, ChatSend, ChatSendRequest, ChatSendResult, ChatSpeaker};
