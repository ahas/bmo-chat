//! The `bmo-chat` command. `bmo-chat mcp --provider <url>` starts the MCP server on standard input and output,
//! driving the provider whose routes stand at the address.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bmo_chat_client::{ChatPatience, ChatProviderClient, RealClock};
use bmo_chat_plugin::ChatServer;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[derive(Parser)]
#[command(name = "bmo-chat", version = bmo_chat_plugin::VERSION, about = "The chat protocol's harness plugin.")]
struct Command {
  #[command(subcommand)]
  verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
  /// Serves the harness tools over MCP on standard input and output, driving one provider.
  Mcp {
    /// The base address the provider's routes stand at.
    #[arg(long, value_name = "url")]
    provider: String,
    /// The provider's bearer token.
    #[arg(long, env = "BMO_CHAT_TOKEN", hide_env_values = true, default_value = "")]
    token: String,
  },
}

#[tokio::main]
async fn main() -> ExitCode {
  let Command { verb } = Command::parse();
  match verb {
    Verb::Mcp { provider, token } => {
      // A harness asks again itself, so a provider nothing listens at is answered at once.
      let once = ChatPatience {
        window: Duration::ZERO,
        every: Duration::ZERO,
        waiter: Arc::new(RealClock),
      };
      let server = ChatServer::new(ChatProviderClient::new(provider, token).with_patience(once));
      let served = match server.serve(stdio()).await {
        Ok(served) => served,
        Err(error) => {
          eprintln!("bmo-chat: the MCP session did not start: {error}");
          return ExitCode::FAILURE;
        }
      };
      match served.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
          eprintln!("bmo-chat: the MCP session ended in a failure: {error}");
          ExitCode::FAILURE
        }
      }
    }
  }
}
