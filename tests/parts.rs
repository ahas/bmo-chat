//! Files carried as parts across the real routes (`docs/protocol.md`, "Files carried as parts"): a file stored
//! on the sender's disk crosses as a part of a multipart body, arrives stored in the receiving end's spool,
//! and is gone from the spool once the receiving end lets it go; a file in memory still crosses inline. Each
//! side is served on a socket of its own, taken down before the test ends.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use bmo_chat::ChatReceiver as _;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-token";
const ROOM: i64 = -100;

/// A directory of this test's own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
  fn new(what: &str) -> Self {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("chat-parts-{what}-{}-{count}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    Self(path)
  }

  fn entries(&self) -> usize {
    std::fs::read_dir(&self.0).unwrap().count()
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// A router served on a socket of its own until `stop` is sent.
struct Served {
  base: String,
  stop: oneshot::Sender<()>,
  listening: JoinHandle<()>,
}

impl Served {
  async fn on(router: Router) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = oneshot::channel::<()>();
    let listening = tokio::spawn(async move {
      axum::serve(listener, router)
        .with_graceful_shutdown(async {
          stopped.await.ok();
        })
        .await
        .expect("the router is served");
    });
    Self {
      base,
      stop,
      listening,
    }
  }

  async fn take_down(self) {
    self.stop.send(()).ok();
    self.listening.await.unwrap();
  }
}

/// What a stand-in was handed: for each file, whether it arrived stored in the spool, and its bytes.
type Handed = Mutex<Vec<(bool, Vec<u8>)>>;

/// A provider that sends documents, telling what each send carried.
struct Provider {
  spool: PathBuf,
  handed: Handed,
}

#[async_trait]
impl bmo_chat::ChatProvider for Provider {
  fn key(&self) -> &str {
    "stand-in"
  }
  fn label(&self) -> &str {
    "Stand-in"
  }
  fn version(&self) -> &str {
    "1.0.0"
  }
  fn configured(&self) -> Option<String> {
    None
  }
  async fn start(&self, _: Arc<dyn bmo_chat::ChatReceiver>) -> Result<(), bmo_chat::ChatFailure> {
    Ok(())
  }
  async fn stop(&self) {}
  fn modes(&self) -> Vec<bmo_chat::ChatReadMode> {
    Vec::new()
  }
  fn can(&self) -> BTreeSet<bmo_chat::ChatCapability> {
    [bmo_chat::ChatCapability::Document].into()
  }
  fn dialect(&self) -> &dyn bmo_chat::ChatDialect {
    &bmo_chat::PlainDialect
  }
  fn numbering(&self) -> bmo_chat::ChatNumbering {
    bmo_chat::ChatNumbering {
      rooms: bmo_chat::ChatNumberSpace::Native,
      people: bmo_chat::ChatNumberSpace::Native,
      messages: bmo_chat::ChatNumberSpace::Native,
      ranges: bmo_chat::ChatNumberRanges::default(),
    }
  }
  fn spool(&self) -> PathBuf {
    self.spool.clone()
  }
  async fn room(&self, _: i64) -> Result<Option<bmo_chat::ProviderRoom>, bmo_chat::ChatFailure> {
    Ok(None)
  }
  async fn owner_room(&self) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    Ok(None)
  }
  async fn send(
    &self,
    request: bmo_chat::ChatSendRequest,
  ) -> Result<bmo_chat::ChatSendResult, bmo_chat::ChatFailure> {
    let bmo_chat::ChatSend::Document { bytes, .. } = request.send else {
      panic!("the stand-in is sent documents alone");
    };
    let stored = matches!(&bytes, bmo_chat::FileBody::Stored(file) if file.path().starts_with(&self.spool));
    let bytes = bytes.read().await?;
    self.handed.lock().unwrap().push((stored, bytes));
    Ok(bmo_chat::ChatSendResult::Sent(bmo_chat::ChatSent {
      message_id: Some(40),
      missed: Vec::new(),
    }))
  }
}

/// A receiving side that holds every message it is handed, telling what each hand-over carried.
struct Receiver {
  spool: PathBuf,
  handed: Handed,
}

#[async_trait]
impl bmo_chat_client::ChatReceiver for Receiver {
  async fn hand_over(
    &self,
    _: i64,
    _: i64,
    message: bmo_chat_client::HandedMessage,
  ) -> Result<bmo_chat_client::HeldMessage, bmo_chat_client::WireError> {
    for file in &message.files {
      let stored = matches!(&file.bytes, bmo_chat_client::FileBody::Stored(file) if file.path().starts_with(&self.spool));
      let bytes = file.bytes.read().await.unwrap();
      self.handed.lock().unwrap().push((stored, bytes));
    }
    Ok(bmo_chat_client::HeldMessage {
      created: true,
      message,
    })
  }
  async fn gathered(
    &self,
    _: i64,
    _: i64,
    _: bmo_chat_client::GatheredPatch,
  ) -> Result<bmo_chat_client::HandedMessage, bmo_chat_client::WireError> {
    panic!("nothing is gathered here");
  }
  async fn avatars_changed(
    &self,
    _: bmo_chat_client::AvatarChange,
  ) -> Result<(), bmo_chat_client::WireError> {
    Ok(())
  }
  async fn reactions_changed(
    &self,
    _: bmo_chat_client::ReactionChange,
  ) -> Result<(), bmo_chat_client::WireError> {
    Ok(())
  }
  async fn deleted(&self, _: bmo_chat_client::Deletion) -> Result<(), bmo_chat_client::WireError> {
    Ok(())
  }
  fn spool(&self) -> PathBuf {
    self.spool.clone()
  }
  async fn room(
    &self,
    _: i64,
    _: bmo_chat_client::RoomQuery,
  ) -> Result<Option<bmo_chat_client::ReceiverRoom>, bmo_chat_client::WireError> {
    Ok(None)
  }
  async fn rooms(
    &self,
    _: bmo_chat_client::RoomsQuery,
  ) -> Result<Vec<bmo_chat_client::ReceiverRoom>, bmo_chat_client::WireError> {
    Ok(Vec::new())
  }
}

/// A megabyte of every byte value in turn, written to `name` in `dir`.
fn source(dir: &Scratch, name: &str) -> (PathBuf, Vec<u8>) {
  let bytes: Vec<u8> = (0..1024 * 1024).map(|index| (index % 251) as u8).collect();
  let path = dir.0.join(name);
  std::fs::write(&path, &bytes).unwrap();
  (path, bytes)
}

#[tokio::test]
async fn a_send_carries_a_stored_file_as_a_part_that_arrives_stored_and_leaves_the_spool_after() {
  let sources = Scratch::new("sources");
  let spool = Scratch::new("provider-spool");
  let (path, bytes) = source(&sources, "report.pdf");
  let provider = Arc::new(Provider {
    spool: spool.0.clone(),
    handed: Mutex::default(),
  });
  let served = Served::on(bmo_chat::provider_router(provider.clone(), TOKEN)).await;
  let client = bmo_chat_client::ChatProviderClient::new(&served.base, TOKEN);
  let document = |bytes: bmo_chat_client::FileBody| {
    let send = bmo_chat_client::ChatSend::Document {
      bytes,
      file_name: "report.pdf".into(),
    };
    bmo_chat_client::ChatSendRequest::new(ROOM, send)
  };

  let stored = client.send(&document(bmo_chat_client::FileBody::stored(&path))).await.unwrap();
  let inline = client.send(&document(vec![1, 2, 3].into())).await.unwrap();
  served.take_down().await;

  assert_eq!((stored.message_id, inline.message_id), (Some(40), Some(40)));
  assert_eq!(*provider.handed.lock().unwrap(), [(true, bytes.clone()), (false, vec![1, 2, 3])]);
  assert_eq!(spool.entries(), 0, "a part let go of is removed from the spool");
  assert_eq!(std::fs::read(&path).unwrap(), bytes, "the sender's own file is left where it lies");
}

#[tokio::test]
async fn a_hand_over_carries_a_stored_file_as_a_part_and_is_answered_with_its_files_withheld() {
  let sources = Scratch::new("sources");
  let spool = Scratch::new("receiver-spool");
  let (path, bytes) = source(&sources, "photo.jpg");
  let receiver = Arc::new(Receiver {
    spool: spool.0.clone(),
    handed: Mutex::default(),
  });
  let served = Served::on(bmo_chat_client::receiver_router(receiver.clone(), TOKEN)).await;
  let link = bmo_chat::ChatReceiverLink::new(&served.base, TOKEN);
  let file = |id: &str, bytes: bmo_chat::FileBody| bmo_chat::MessageFile {
    id: id.into(),
    name: "photo.jpg".into(),
    mime: "image/jpeg".into(),
    bytes,
    kind: Some(bmo_chat::ChatFileKind::Photo),
  };
  let message = bmo_chat::HandedMessage {
    origin: bmo_chat::MessageOrigin::Live,
    sender_id: Some(42),
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000_000,
    text: "two photos".into(),
    reply_to_message_id: None,
    via: "stand-in".into(),
    album: None,
    mode: bmo_chat::ChatVoice::BotAccount,
    context: bmo_chat::InboundChatContext::default(),
    room: bmo_chat::RoomFacts::default(),
    files: vec![file("0", bmo_chat::FileBody::stored(&path)), file("1", vec![9].into())],
    gathering: false,
    gather_failure: None,
  };

  let held = link.hand_over(ROOM, 7, message).await.unwrap();
  served.take_down().await;

  assert!(held.created);
  let ids: Vec<_> = held
    .message
    .files
    .iter()
    .map(|file| (file.id.as_str(), file.bytes.is_withheld()))
    .collect();
  assert_eq!(ids, [("0", true), ("1", true)], "the answer names each file without its bytes");
  assert_eq!(*receiver.handed.lock().unwrap(), [(true, bytes.clone()), (false, vec![9])]);
  assert_eq!(spool.entries(), 0, "a part let go of is removed from the spool");
  assert_eq!(std::fs::read(&path).unwrap(), bytes, "the provider's own file is left where it lies");
}
