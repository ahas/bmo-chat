//! Prints one side's OpenAPI document as the repository keeps it:
//! `cargo run -p bmo-chat-plugin --example openapi -- provider > docs/openapi/provider.json`, and the same with
//! `receiver` for `docs/openapi/receiver.json`.

use std::process::ExitCode;

use bmo_chat_plugin::{Side, openapi_json};

fn main() -> ExitCode {
  let named = std::env::args().nth(1).unwrap_or_default();
  let Some(side) = Side::named(&named) else {
    eprintln!("Name the side whose document to print: provider or receiver.");
    return ExitCode::FAILURE;
  };
  print!("{}", openapi_json(side));
  ExitCode::SUCCESS
}
