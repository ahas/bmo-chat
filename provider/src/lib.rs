//! The chat provider side of the chat protocol.
//!
//! A chat service imports this crate to be a provider: the chat piece types, the provider contract, the
//! server for the provider routes and the link that hands what a room said to the receiver. The pieces are
//! stated in `docs/contract.md` and the routes in `docs/protocol.md`.

mod bytes;
mod capability;
mod declaration;
mod dialect;
mod error;
mod media;
mod message;
mod modes;
mod numbering;
mod plain;
mod provider;
mod refusal;
mod send;

pub use capability::{ChatCapability, ChatVerb};
pub use declaration::ChatDeclaration;
pub use dialect::{ChatDialect, ChatDialectDeclaration, ChatDialectLimits, ChatMarkup};
pub use error::{ErrorCode, WireError};
pub use media::{ChatMediaRequest, ChatMediaResult};
pub use message::{
  ChatGathered, ChatHandover, ChatMessage, FileItem, GatheredOutcome, InboundChatContext, RoomFacts,
  RoomKind,
};
pub use modes::{ChatModeStanding, ChatReach, ChatReadMode, ChatVoice};
pub use numbering::{ChatNumberSpace, ChatNumbering};
pub use plain::PlainDialect;
pub use provider::{ChatFailure, ChatProvider, ChatReceiver};
pub use refusal::{ChatRefusal, MissedPicture, MissedPictureReason};
pub use send::{AlbumItem, ChatSend, ChatSendRequest, ChatSendResult, ChatSpeaker};
