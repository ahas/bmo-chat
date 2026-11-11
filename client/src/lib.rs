//! The receiving side of the chat protocol.
//!
//! The receiving side imports this crate to reach every chat provider through one client and to serve the
//! routes a provider hands messages to; a harness reaches a provider through the same client. The pieces
//! are stated in `docs/contract.md` and the routes in `docs/protocol.md`. This crate defines every wire shape
//! on its own; a test at the repository root checks that they agree with the provider side's.

mod avatar;
mod bodies;
mod bytes;
mod capability;
mod contacts;
mod declaration;
mod dialect;
mod error;
mod inbound;
mod media;
mod message;
mod missed;
mod modes;
mod numbering;
mod patience;
mod provider_client;
mod receiver_bodies;
mod send;

pub use avatar::{AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
pub use bodies::{
  AccountRoomsQuery, ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, Page,
  ProviderOwner, ProviderRoom,
};
pub use capability::{ChatCapability, ChatVerb};
pub use contacts::{ChatContact, ChatContactKind, ChatStart, ContactsQuery};
pub use declaration::ChatDeclaration;
pub use dialect::{ChatDialectDeclaration, ChatDialectLimits, ChatMarkup};
pub use error::{ErrorCode, RETRY_AFTER_SECONDS, WireError};
pub use inbound::{ChatReceiver, receiver_router};
pub use media::{ChatMediaRequest, FilesQuery};
pub use message::{
  ChatFileKind, FileItem, GatheredPatch, HandedMessage, InboundChatContext, MessageFile, MessageOrigin,
  RoomFacts, RoomKind,
};
pub use missed::{MissedPicture, MissedPictureReason};
pub use modes::{ChatModeStanding, ChatReach, ChatReadMode, ChatVoice};
pub use numbering::{ChatNumberRange, ChatNumberRanges, ChatNumberSpace, ChatNumbering};
pub use patience::{ChatPatience, ChatWait, RealClock};
pub use provider_client::{ChatProviderClient, ClientFailure};
pub use receiver_bodies::{
  AvatarChange, Deletion, HeldMessage, MessageStamp, PersonReactions, ReactionChange, ReceiverRoom,
  RoomQuery, RoomsQuery,
};
pub use send::{AlbumItem, ChatSend, ChatSendRequest, ChatSent};
