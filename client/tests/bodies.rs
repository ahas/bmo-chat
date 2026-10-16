//! The bodies of health, the dialect routes and the room routes cross the wire in the shape
//! `docs/protocol.md` states, nullable fields as null, and come back as they went.

use bmo_chat_client::{
  ChatHealth, ChatMarkup, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom,
  RoomIdentity, RoomKind, RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
use serde_json::{Value, json};

fn crosses<T>(value: &T, wire: Value)
where
  T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
  assert_eq!(serde_json::to_value(value).expect("the value encodes"), wire);
  assert_eq!(&serde_json::from_value::<T>(wire).expect("the wire body decodes"), value);
}

#[test]
fn health_ready_and_not() {
  crosses(&ChatHealth::ready(), json!({ "ready": true, "reason": null }));
  crosses(
    &ChatHealth::not_ready("No token is configured."),
    json!({ "ready": false, "reason": "No token is configured." }),
  );
}

#[test]
fn room_query_and_every_room_answer() {
  crosses(&RoomQuery { chat_id: -100 }, json!({ "chatId": -100 }));
  crosses(&RoomOwns { owns: true }, json!({ "owns": true }));
  crosses(&RoomIdentity { identity: Some(7) }, json!({ "identity": 7 }));
  crosses(&RoomIdentity { identity: None }, json!({ "identity": null }));
  crosses(
    &RoomKindAnswer {
      kind: RoomKind::Owner,
    },
    json!({ "kind": "owner" }),
  );
  crosses(
    &RoomKindAnswer {
      kind: RoomKind::Group,
    },
    json!({ "kind": "group" }),
  );
  crosses(&RoomPerson { person: Some(42) }, json!({ "person": 42 }));
  crosses(&RoomPerson { person: None }, json!({ "person": null }));
  crosses(
    &RoomTitle {
      title: Some("A Room".into()),
    },
    json!({ "title": "A Room" }),
  );
  crosses(&RoomTitle { title: None }, json!({ "title": null }));
  crosses(
    &OwnerRoom {
      chat_id: Some(-100),
    },
    json!({ "chatId": -100 }),
  );
  crosses(&OwnerRoom { chat_id: None }, json!({ "chatId": null }));
}

#[test]
fn a_room_query_names_what_it_misses() {
  let error = serde_json::from_value::<RoomQuery>(json!({})).unwrap_err();
  assert!(error.to_string().contains("missing field `chatId`"), "{error}");
}

#[test]
fn dialect_read_and_its_reading() {
  crosses(
    &DialectRead {
      text: "**hello**".into(),
    },
    json!({ "text": "**hello**" }),
  );
  crosses(
    &DialectReading {
      markup: ChatMarkup {
        markup: "<b>hello</b>".into(),
        plain: "hello".into(),
        formatted: true,
      },
      parts: vec!["**hello**".into()],
      fold: Some("<details>hello</details>".into()),
    },
    json!({
      "markup": "<b>hello</b>", "plain": "hello", "formatted": true, "parts": ["**hello**"],
      "fold": "<details>hello</details>",
    }),
  );
  crosses(
    &DialectReading {
      markup: ChatMarkup {
        markup: "hello".into(),
        plain: "hello".into(),
        formatted: false,
      },
      parts: vec!["hello".into()],
      fold: None,
    },
    json!({ "markup": "hello", "plain": "hello", "formatted": false, "parts": ["hello"], "fold": null }),
  );
}

#[test]
fn dialect_check_and_its_answer() {
  crosses(
    &DialectCheck {
      markup: "<b>open".into(),
    },
    json!({ "markup": "<b>open" }),
  );
  crosses(
    &DialectChecked {
      fault: Some("A tag is left open.".into()),
      visible: "open".into(),
    },
    json!({ "fault": "A tag is left open.", "visible": "open" }),
  );
  crosses(
    &DialectChecked {
      fault: None,
      visible: "shut".into(),
    },
    json!({ "fault": null, "visible": "shut" }),
  );
}
