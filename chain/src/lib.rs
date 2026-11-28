//! A client of the account service's delegation chain (`docs/chain.md`): every unit that judges what a person or a
//! unit may do asks the chain the same way through it. The client asks and reads; which kinds a unit declares,
//! what an undeclared kind or an unreachable service means, and the value that stands where the chain says
//! nothing are the unit's own, so it takes no fallback of its own and keeps nothing between calls.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

/// Whom a kind is resolved for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
  Person(i64),
  Unit(i64),
}

impl Subject {
  fn query(self) -> (&'static str, String) {
    match self {
      Self::Person(person) => ("person", person.to_string()),
      Self::Unit(unit) => ("unit", unit.to_string()),
    }
  }
}

/// What the chain answered a kind's resolution.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
  /// What its links said for the subject, as the service answered it.
  Resolved(Value),
  /// The kind is declared by no unit.
  Undeclared,
  /// The subject does not stand on the chain.
  Nothing,
}

/// What the chain answered a call made in a caller's name: its status and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct Answered {
  pub status: u16,
  pub body: Value,
}

impl Answered {
  /// Whether the call was taken.
  pub fn ok(&self) -> bool {
    (200..300).contains(&self.status)
  }
}

/// The account service's chain as one unit asks it: its address, the service token, and the unit's own name as
/// the kinds it declares name it.
#[derive(Clone, Debug)]
pub struct Chain {
  address: String,
  token: String,
  declared_by: String,
  client: reqwest::Client,
}

impl Chain {
  /// The chain at `address`, asked with `token` by the unit `declared_by` names, each call waiting at most `wait`.
  pub fn new(address: &str, token: &str, declared_by: &str, wait: Duration) -> Self {
    Self {
      address: address.trim_end_matches('/').to_owned(),
      token: token.to_owned(),
      declared_by: declared_by.to_owned(),
      client: reqwest::Client::builder().timeout(wait).build().unwrap_or_default(),
    }
  }

  /// Whether an account service is named at all; a unit that names none asks nothing.
  pub fn named(&self) -> bool {
    !self.address.is_empty()
  }

  /// Each kind declared, `PUT /chain/kinds/{key}`, as the unit's own body for it, its `declaredBy` the unit's
  /// name; the first refusal or failure is the error.
  pub async fn declare(&self, kinds: &[Value]) -> Result<(), String> {
    for kind in kinds {
      let key = kind["key"].as_str().unwrap_or_default();
      let mut body = kind.clone();

      body["declaredBy"] = json!(self.declared_by);

      let answer = self
        .client
        .put(format!("{}/chain/kinds/{key}", self.address))
        .bearer_auth(&self.token)
        .json(&body)
        .send()
        .await
        .map_err(unreached)?;
      let status = answer.status();

      if !status.is_success() {
        let said = answer.text().await.unwrap_or_default();

        return Err(format!("the account service refused the kind {key} with {}: {said}", status.as_u16()));
      }
    }

    Ok(())
  }

  /// One kind resolved for a subject, `GET /chain/kinds/{key}/resolve`.
  pub async fn resolve(&self, key: &str, subject: Subject) -> Result<Resolution, String> {
    let answer = self
      .client
      .get(format!("{}/chain/kinds/{key}/resolve", self.address))
      .bearer_auth(&self.token)
      .query(&[subject.query()])
      .send()
      .await
      .map_err(unreached)?;
    let status = answer.status();

    if status == StatusCode::NOT_FOUND {
      let body: Value = answer.json().await.unwrap_or(Value::Null);

      return Ok(if body["code"] == "no-such-kind" {
        Resolution::Undeclared
      } else {
        Resolution::Nothing
      });
    }

    if !status.is_success() {
      return Err(format!("the account service answered {}", status.as_u16()));
    }

    answer
      .json()
      .await
      .map(Resolution::Resolved)
      .map_err(|why| format!("the account service's answer does not read: {why}"))
  }

  /// A link's entry of a kind, read in the caller's name: the chain answers it to whoever writes the link and to a
  /// person at their own, and refuses anyone else 403.
  pub async fn entry(&self, key: &str, link: &str, caller: i64) -> Result<Answered, String> {
    let request = self.client.get(format!("{}/chain/kinds/{key}/links/{link}", self.address));

    self.in_name_of(request, Some(caller)).await
  }

  /// A link's value written in the caller's name, null to follow the link above, the unit named as the one that
  /// declared the kind; the chain judges whether the caller writes it, and whether a lock above withholds it.
  pub async fn write_value(
    &self,
    key: &str,
    link: &str,
    caller: i64,
    value: Value,
  ) -> Result<Answered, String> {
    let request = self
      .client
      .put(format!("{}/chain/kinds/{key}/links/{link}/value", self.address))
      .header("Declared-By", &self.declared_by)
      .json(&json!({ "value": value }));

    self.in_name_of(request, Some(caller)).await
  }

  /// A link's delegation written in the caller's name, `allow`, `lock` or `inherit`.
  pub async fn write_delegation(
    &self,
    key: &str,
    link: &str,
    caller: i64,
    delegation: &str,
  ) -> Result<Answered, String> {
    let request = self
      .client
      .put(format!("{}/chain/kinds/{key}/links/{link}/delegation", self.address))
      .json(&json!({ "delegation": delegation }));

    self.in_name_of(request, Some(caller)).await
  }

  /// A link's rules written whole in the caller's name, each `who` and `value`.
  pub async fn write_rules(
    &self,
    key: &str,
    link: &str,
    caller: i64,
    rules: Value,
  ) -> Result<Answered, String> {
    let request = self
      .client
      .put(format!("{}/chain/kinds/{key}/links/{link}/rules", self.address))
      .json(&json!({ "rules": rules }));

    self.in_name_of(request, Some(caller)).await
  }

  /// A read of the service at `path`, in the caller's name where one is named: a unit, its subtree or a group.
  pub async fn read(&self, path: &str, caller: Option<i64>) -> Result<Answered, String> {
    let request = self.client.get(format!("{}{path}", self.address));

    self.in_name_of(request, caller).await
  }

  // A call with the service token, in the caller's name where one is named, its answer whole.
  async fn in_name_of(
    &self,
    request: reqwest::RequestBuilder,
    caller: Option<i64>,
  ) -> Result<Answered, String> {
    let request = request.bearer_auth(&self.token);
    let request = match caller {
      Some(caller) => request.header("Owner-Key", caller.to_string()),
      None => request,
    };
    let answer = request.send().await.map_err(unreached)?;
    let status = answer.status().as_u16();
    let body = answer.json().await.unwrap_or(Value::Null);

    Ok(Answered { status, body })
  }
}

fn unreached(why: reqwest::Error) -> String {
  format!("the account service was not reached: {why}")
}

/// A resolved flag's value, `fallback` where the answer says none: the unit names what stands where the chain says
/// nothing.
pub fn flag(answer: Option<&Value>, fallback: bool) -> bool {
  answer.and_then(|answer| answer["value"].as_bool()).unwrap_or(fallback)
}

/// The units on a resolved subject's path, the farthest first, as its answer's layers name them.
pub fn units_of(answer: &Value) -> Vec<i64> {
  answer["layers"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|layer| layer["link"].as_str()?.strip_prefix("unit:")?.parse().ok())
    .collect()
}

/// The links of a resolved subject's chain, top first, as its answer's path names them.
pub fn path_of(answer: &Value) -> Vec<String> {
  answer["path"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|link| link.as_str().or_else(|| link["link"].as_str()).map(str::to_owned))
    .collect()
}
