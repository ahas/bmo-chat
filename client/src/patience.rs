//! How long a request whose connection was refused keeps being offered, as `docs/protocol.md` states under
//! "Retrying".

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

/// How a wait between two offers of a request is spent. The client counts the time it has waited by the
/// intervals it asked for, never by reading a clock, so a waiter that returns at once makes a whole window
/// pass at once.
#[async_trait]
pub trait ChatWait: Send + Sync {
  async fn wait(&self, interval: Duration);
}

/// Waits on the real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealClock;

#[async_trait]
impl ChatWait for RealClock {
  async fn wait(&self, interval: Duration) {
    tokio::time::sleep(interval).await;
  }
}

/// How long a request whose connection was refused keeps being offered, and how often.
#[derive(Clone)]
pub struct ChatPatience {
  /// The most time spent waiting before the request is given up.
  pub window: Duration,
  /// The wait before each new offer; zero offers a request only once.
  pub every: Duration,
  pub waiter: Arc<dyn ChatWait>,
}

impl Default for ChatPatience {
  /// Two minutes, offered every two seconds, on the real clock.
  fn default() -> Self {
    Self {
      window: Duration::from_secs(120),
      every: Duration::from_secs(2),
      waiter: Arc::new(RealClock),
    }
  }
}

impl fmt::Debug for ChatPatience {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ChatPatience")
      .field("window", &self.window)
      .field("every", &self.every)
      .finish_non_exhaustive()
  }
}
