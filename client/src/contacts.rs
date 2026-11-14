//! The contacts of a provider's account, and a conversation started with them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What kind of account a contact is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatContactKind {
  /// A person's account.
  Person,
  /// A platform's bot account.
  Bot,
}

/// One contact of the provider's account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatContact {
  /// The contact, as a number of the provider's `people` space.
  pub id: i64,
  /// What the account calls the contact.
  pub name: String,
  pub kind: ChatContactKind,
  /// The one-to-one the account already has with the contact, or null where it has none.
  pub room_id: Option<i64>,
}

/// The query of `GET /contacts`: only the contacts numbered after `after`, and at most `limit` of them, of the
/// account `accountId` names, or of the provider's own where it names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContactsQuery {
  /// A contact number.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
  /// The account the listing is for.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account_id: Option<i64>,
}

/// A conversation to start, with one contact or with several under a title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum ChatStart {
  /// A one-to-one with one contact of the account.
  #[serde(rename_all = "camelCase")]
  OneToOne { contact_id: i64 },
  /// A room of several contacts, under a title: not blank, and naming each contact once.
  #[serde(rename_all = "camelCase")]
  Group { title: String, contact_ids: Vec<i64> },
}

/// The body of `POST /rooms`: a conversation to start, and the account it is for. The start's shapes cannot
/// close themselves once flattened beside `accountId`, so the body is closed here: a member neither the shape
/// nor this request reads makes it no start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(extend("unevaluatedProperties" = false))]
pub struct ChatStartRequest {
  #[serde(flatten)]
  pub start: ChatStart,
  /// The account the start is for; unnamed, the provider's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account_id: Option<i64>,
}

impl ChatStartRequest {
  /// A start for the provider's own account.
  pub fn new(start: ChatStart) -> Self {
    Self {
      start,
      account_id: None,
    }
  }
}
