//! The bodies of the dialect routes cross the wire in the shape `docs/protocol.md` states, nullable fields as
//! null, and come back as they went; a query names the field it misses.

use bmo_chat_client::{ChatMarkup, DialectCheck, DialectChecked, DialectRead, DialectReading, RoomQuery};
use serde_json::{Value, json};

fn crosses<T>(value: &T, wire: Value)
where
  T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
  assert_eq!(serde_json::to_value(value).expect("the value encodes"), wire);
  assert_eq!(&serde_json::from_value::<T>(wire).expect("the wire body decodes"), value);
}

#[test]
fn a_query_names_what_it_misses() {
  let error = serde_json::from_value::<RoomQuery>(json!({})).unwrap_err();
  assert!(error.to_string().contains("missing field `via`"), "{error}");
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
