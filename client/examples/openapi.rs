//! Prints the protocol's OpenAPI document as the repository keeps it:
//! `cargo run -p bmo-chat-client --example openapi > docs/openapi.json`.

fn main() {
  print!("{}", bmo_chat_client::openapi_json());
}
