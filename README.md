# chat

The chat protocol's base libraries. Chat services (providers) and the receiver are separate services that
reach each other over HTTP with JSON bodies; these two crates are the two ends of that wire.

| Crate | Directory | Imported by | Supplies |
|---|---|---|---|
| `bmo-chat` | `provider/` | a chat service | the chat piece types, the provider contract, the server for the provider routes and the link to the receiver |
| `bmo-chat-client` | `client/` | the receiver | one client for every provider, and the server for the routes a provider hands messages to |

Neither crate depends on the other. Each states the wire shapes on its own, and the root package's tests
check that the two agree.

## Documents

- `docs/contract.md`: the piece types, such as messages, sends, refusals, dialects and the provider's
  declaration.
- `docs/protocol.md`: the routes in both directions, their bodies and answers, the error shape and the
  retry rule.

## Layout

- `provider/`, `client/`: the two crates, each with its own `src/` and `tests/`.
- `tests/`: checks that span the repository, run as the root package's tests.
- `deploy.toml`: the build and test commands. Its format is provisional until bmo-testbed defines the
  convention; the repository is a library and has no deploy command.

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets
```
