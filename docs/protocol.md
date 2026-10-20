# The chat wire protocol

Providers and the receiving side are separate services and reach each other over HTTP with JSON bodies.
Every route below is part of the protocol. A route that is not here is not part of the base, whatever service
answers it. The pieces the bodies carry (messages, sends, refusals, declarations) are stated in `contract.md`,
and the wire format and API design every route follows in `conventions.md`.

There are two directions:

- **Provider routes**: a provider serves them (bmo-chat supplies the server) and the receiving side calls
  them (bmo-chat-client supplies the client). One client reaches every provider at the base address the
  receiving side's own configuration gives for it.
- **Receiving-side routes**: the receiving side serves them (bmo-chat-client supplies the server) and a
  provider calls them (bmo-chat supplies the link). The provider's own configuration gives the receiving
  side's base address.

Paths below are relative to that base address. A service may mount the routes under a prefix of its own.

This document states version 0.1.0-rev.5 of the libraries and the plugin; a caller pins that version or a
later one for every route and field below. The version is numbered by the revision rule of `conventions.md`.
Each older number the crates carry in their history stands for one revision:

| Older number | Revision    |
| ------------ | ----------- |
| 0.1.0        | 0.1.0       |
| 0.2.0        | 0.1.0-rev.1 |
| 0.3.0        | 0.1.0-rev.2 |
| 0.4.0        | 0.1.0-rev.3 |
| 0.5.0        | 0.1.0-rev.4 |
| 1.0.0        | 0.1.0-rev.5 |

## The OpenAPI documents

Each direction is stated in OpenAPI 3.1 for a service written in any language, in its own document beside
this one, since the two sides serve paths of the same shape: `openapi/provider.json` for the provider routes
and `openapi/receiver.json` for the receiving-side routes. Each states every route's method and path, its
parameters, its request body, its answers and the error codes it can give, with every body as a JSON Schema.
A body is described as its sender writes it, so a field the sender may leave out is not required, and a
nullable field is required and may be null. The provider document also carries the harness plugin's tools
under `x-tools` and the plugin itself under `x-harness`.

bmo-chat-plugin generates both documents from the types bmo-chat-client sends and reads and from the tools
its MCP server lists, and its tests fail when a committed file differs from what it generates.
`cargo run -p bmo-chat-plugin --example openapi -- provider > docs/openapi/provider.json` writes the provider
document again, and the same with `receiver` the receiving side's. This document is the canonical statement:
where it and a generated document disagree, the code is repaired and the documents generated again.

## Conventions

The wire format is the common one of `conventions.md`: camelCase keys, times as integers of epoch
milliseconds, rooms, people and messages as signed 64-bit integers, lists as `{"items", "more"}` pages, and
every failure in the error shape. Beside it:

- **Optional and nullable fields.** An optional field may be left out. A field described as nullable is sent
  as null when it has no value.
- **Bytes.** Bytes inside a JSON body are base64 strings (standard alphabet, with padding). A profile picture
  is the one answer that is not JSON: it is answered as its own bytes, typed by `Content-Type`.
- **Auth.** Every route, on both sides, needs an `Authorization: Bearer <token>` header. The token is supplied
  by the service that serves the routes, from its own configuration; the libraries read no environment
  variable themselves. A caller is configured with the token of the service it calls. A request with a
  missing or wrong token is answered 401.
- **Refusals are errors.** A send the provider does not perform and a file it cannot fetch are answered with a
  non-2xx status in the error shape, like a request that could not be answered at all. A 2xx answer is
  always the route's success.

### Error codes

Every non-2xx answer is the error shape of `conventions.md`: `code` and `error`, an English sentence saying
what went wrong. A code means what that document's table says it means; the protocol answers these, beside
its refusals:

| Status | `code` | When |
|---|---|---|
| 400 | `bad-request` | the body is not JSON, or the body or the query is not the shape the route takes; the sentence names what is wrong |
| 401 | `unauthorized` | the token is missing or wrong |
| 404 | `no-route` | no route answers this method and path, a path key that is not a number among them |
| 404 | `not-found` | the route is right, but nothing stands at the key the path names (a room the provider knows nothing of, a profile picture it has not kept, a message the receiving side does not hold) |
| 409 | `conflict` | the receiving side holds the member, and its state does not allow the change |
| 500 | `failed` | the service failed while answering; the sentence is the failure |
| 503 | `not-ready` | the service is up but cannot answer yet (a provider whose platform connection is not open yet); `Retry-After` says when to ask again |

Both libraries carry every code `conventions.md` lists, so a receiving side may answer any of them, and so may
a route a service serves of its own beside the protocol's. bmo-chat's error shape answers as an HTTP response
with its code's status, `Retry-After` included where the code is `not-ready`, so such a route returns it as it
is.

### Refusals

A refusal of a send or of a file fetch is answered in the error shape. Its `code` is the refusal's kind as
`contract.md` lists it, and `error` is the refusal's English sentence as the provider rendered it, which
carries the refusal's own fields (the capability, the speaker, each missed picture with its reason). No other
key crosses.

| Status | `code` | When |
|---|---|---|
| 403 | `speaker-not-allowed` | the send names a speaker, and the provider speaks as the room's own account alone |
| 404 | `unavailable` | the platform would not give the files a fetch asked for; the sentence says why |
| 410 | `gone` | the message the call is about is one the platform no longer has, or will never let change again, so the caller lets go of it instead of asking again |
| 422 | `unsupported-capability` | the provider does not declare the capability the send or the call needs |
| 422 | `no-picture-sent` | none of the pictures of a send went |
| 422 | `speaker-unnumbered` | the provider has no number in the room for the speaker the send names |
| 422 | `platform` | the platform refused, for the provider's own English reason |
| 503 | `not-ready` | the provider is not ready yet: the same code and status as the error above |

### Retrying

A caller offers a request again only when the connection was refused, meaning nothing listened and the request
cannot have been received. It keeps offering within a patience window, which covers the time a service takes
to restart for a deployment. Two minutes, offered every two seconds, is the default. The window and the
interval can be set by the caller, so a test never waits on the real clock. A request that reached a listener
is never offered again by the libraries, whatever it was answered, so a send is never made twice; a caller
answered `not-ready` decides itself whether to ask again after `Retry-After`.

## Provider routes

| Method | Path | Answer |
|---|---|---|
| `GET` | `/declaration` | 200, the provider's declaration |
| `GET` | `/health` | 200, whether the provider is ready |
| `POST` | `/sends` | 201, what the send came to |
| `GET` | `/rooms/{roomId}` | 200, what the provider knows of a room |
| `GET` | `/owner` | 200, the room to reach the account's owner in |
| `GET` | `/rooms/{roomId}/messages/{messageId}/files` | 200, a page of the files a message carried |
| `POST` | `/dialect/readings` | 200, the dialect's reading of markdown |
| `POST` | `/dialect/checks` | 200, the dialect's check of markup |
| `GET` | `/avatars` | 200, a page of the profile pictures kept |
| `GET` | `/rooms/{roomId}/avatar` | 200, a room's kept picture as its bytes |
| `GET` | `/people/{personId}/avatar` | 200, a person's kept picture as its bytes |

The declaration, health, dialect and avatar routes answer whatever state the provider is in. The others
(sends, a room, the owner and a message's files) answer 503 `not-ready` while the provider's health is not
ready, without asking the provider.

### `GET /declaration`

The provider's self-declaration (`contract.md`, "The provider's declaration"). The answer:

| Field | Type |
|---|---|
| `key` | string |
| `label` | string |
| `configured` | string or null: null when the provider can run, else why not |
| `modes` | list of read modes: `voice`, `reach`, `standing`, `reason` (string or null), `canReplyTo` (boolean) |
| `can` | list of capabilities |
| `dialect` | `name` (string), `limits` (`text`, `caption`, `albumPhotos`, `fileBytes`, each a number or null), `writing` (string), `foldable` (boolean) |
| `shows` | string or null |
| `numbering` | `rooms`, `people`, `messages`, each a number space; `ranges`: `rooms` and `people`, each a range (`first`, `last`, numbers, both included) or null |
| `speakers` | boolean |

A mode's standing can change while the provider runs, so a caller asks again instead of keeping an old
answer.

### `GET /health`

Whether the provider is ready. The answer is `ready` (boolean), `reason` (string or null: why not, where it is
not ready) and `version` (string: the provider service's own version, in semver).

### `POST /sends`

One send. The body is a send as `contract.md` states it:

| Field | Type |
|---|---|
| `kind` | the send's kind: `text`, `photo`, `album`, `document`, `audio`, `voice`, `sticker`, `reaction`, `pin`, `typing`, `edit` or `delete` |
| `roomId` | number: the room the send goes into |
| `fields` | object: the kind's own fields, as `contract.md` lists them |
| `mode` | optional voice to speak with |
| `speaker` | optional string: who speaks, where it is not the one the room is answered by |
| `tag` | optional string returned on the platform's echo of the send |
| `prefix` | optional string put before the words in the `human-avatar` voice |
| `replyToMessageId` | optional number: the message the send answers |

Bytes (`bytes` of a document, `audio` of an audio or a voice, `bytes` of an inline album item) are base64.

The answer, for a send the provider performed, is 201 with `messageId` (number or null) and `missed`, the
list of the pictures of an album that did not go, each `address` and `reason`, empty where everything went. A
missed picture's `reason` is an object with `code`, `message` (the English sentence) and, for `platform`,
`detail`.

A send the provider does not perform is answered with a refusal (see Refusals). A send of a kind the provider
does not declare is refused `unsupported-capability` by the base before the provider is asked. A provider that
fails while sending answers 500 `failed`.

### `GET /rooms/{roomId}`

What the provider knows of a room (`contract.md`, "A room as a provider knows it"). The answer:

| Field | Type |
|---|---|
| `id` | number: the room |
| `owns` | boolean: whether the room is one of this provider's |
| `identity` | number or null: the sender number the account's own words carry in the room |
| `kind` | `owner` or `group` |
| `privateTo` | number or null: the one person in a one-to-one with the account |
| `title` | string or null: what the platform calls the room |

A room the provider knows nothing of is answered 404 `not-found`.

### `GET /owner`

The account's owner, as far as the provider can reach them. The answer is `roomId`: number or null, the room to
speak into to reach the owner unprompted.

### `GET /rooms/{roomId}/messages/{messageId}/files`

The pictures and files a message carried, fetched again from the platform. Gated by the `fetchMedia`
capability. The query takes `sentAt` (number, epoch milliseconds), optional, for a platform that finds an old
message by its time. The answer is a page whose `items` are message files, each `id`, `name`, `mime`, `bytes`
(base64) and optionally `kind`, in the order the message carried them; `more` is always false, since a
message's files come in one page.

Files the platform would not give are answered 404 `unavailable`, and a fetch the provider refuses is
answered with its refusal (see Refusals). A provider that does not declare `fetchMedia` is refused
`unsupported-capability` by the base before the provider is asked.

### `POST /dialect/readings`

The provider's dialect reading markdown. The body is `text` (the markdown). The answer:

| Field | Type |
|---|---|
| `markup` | string: what would go out |
| `plain` | string: the same words with the marks stripped |
| `formatted` | boolean |
| `parts` | list of strings: the markdown divided into pieces that each fit one message |
| `fold` | string or null: the folded markup, or null where the dialect does not fold or there is nothing worth folding |

### `POST /dialect/checks`

The provider's dialect checking markup it built. The body is `markup`. The answer is `fault` (string or null:
null when the markup is sound, else the reason) and `visible` (string: what a reader sees of it).

### `GET /avatars`

The profile pictures the provider keeps (`contract.md`, "Profile pictures"). The query takes `after` (a time)
and `limit`, each optional. The answer is a page whose `items` are the pictures, each `kind` (`room` or
`person`), `id` (number) and `takenAt` (number: when the picture was taken), oldest taken first and only those
taken after `after`; `more` says whether pictures stand past the last one, and the next page is asked with its
`takenAt` as `after`. A page never ends between two pictures taken at the same moment, so it holds more than
`limit` items where `limit` would part them. A provider that does not declare `avatars` answers an empty page.

### `GET /rooms/{roomId}/avatar`, `GET /people/{personId}/avatar`

The picture the provider keeps for a room or a person by its number. The answer is the picture's bytes, with
its type as `Content-Type`. Where none is kept, or the provider does not declare `avatars`, the answer is 404
`not-found`.

## Receiving-side routes

| Method | Path | Answer |
|---|---|---|
| `GET` | `/rooms` | 200, a page of the provider's rooms whose past it may read back |
| `GET` | `/rooms/{roomId}` | 200, one room as the receiving side holds it |
| `PUT` | `/rooms/{roomId}/messages/{messageId}` | 201 or 200, the message handed over, as held |
| `PATCH` | `/rooms/{roomId}/messages/{messageId}` | 200, the message with the files its gathering promised |
| `POST` | `/avatar-changes` | 201, word that kept pictures changed |

Every receiving-side route can answer 503 `not-ready` where the receiving side cannot take or answer it yet,
and 500 `failed` where it failed while doing so, beside the codes each route names.

### `GET /rooms`

The rooms of one provider whose past it may read back, each with the oldest message the receiving side holds
of it. The query takes `via` (the key of the provider asking), required, and `after` (a room number) and
`limit`, each optional. The answer is a page whose `items` are rooms in ascending order of `id`, only those
after `after`, each as `GET /rooms/{roomId}` answers it; `more` says whether rooms stand past the last one.

### `GET /rooms/{roomId}`

One room as the receiving side holds it. The query takes `via`, the key of the provider asking, required.
The answer:

| Field | Type |
|---|---|
| `id` | number: the room |
| `listened` | boolean: whether the receiving side listens to the room |
| `oldestMessageId` | number or null: the oldest message held of the room, null where none is |
| `oldestSentAt` | number or null: when that message was sent, null where none is |

A room the receiving side holds nothing of is answered 404 `not-found`, which a provider reads as a room not
listened to.

### `PUT /rooms/{roomId}/messages/{messageId}`

A message handed over: said live, changed, or read back from the room's past. The room and the message are
the path's; the body is the message as `contract.md` states it:

| Field | Type |
|---|---|
| `origin` | `live`, `edit` or `history` |
| `senderId` | number or null |
| `senderName` | string |
| `sentAt` | number |
| `text` | string |
| `replyToMessageId` | number or null |
| `via` | string: the key of the provider that took the message in |
| `album` | number or null |
| `mode` | the voice of the mode that delivered it |
| `context` | object: `roomTitle`, `senderUsername`, `replyToSenderId`, `spokenText`, `metadata` (any JSON), `carriesFile`, `tag`, each optional |
| `room` | object: `kind` (`owner` or `group`), `identity` (number or null), `privateTo` (number or null), each optional |
| `files` | list of message files, each `id`, `name`, `mime`, `bytes` (base64) and optionally `kind`: the files the message carries, where they are already gathered |
| `gathering` | boolean: the provider is still gathering the message's files and sends them with a `PATCH` |
| `gatherFailure` | optional string or null: why the files gathering promised could not be kept |

The answer is the message as the receiving side holds it after the hand-over: 201 where it did not hold the
message before, 200 where it did.

- A `live` message is one a room said. One the receiving side already holds is not taken again.
- An `edit` replaces the message the receiving side holds: its words and its sender (`senderId`,
  `senderName`) are taken as the hand-over carries them.
- A `history` message is kept where the receiving side does not hold it yet, and wakes nobody and routes
  nothing. One already held is left as it is, and the answer is 200.

A message whose files are still on their way says `gathering: true`, carries no `files`, and is followed by a
`PATCH` of the same message.

A message file's `id` is a string, required, the same each time the same file crosses and never shared by two
files of one message (`contract.md`, "Files"). An inline album item of a send is a file item and carries no
`id`.

### `PATCH /rooms/{roomId}/messages/{messageId}`

The files a message's `gathering` promised, or why they could not be kept, as a JSON merge patch of the
message. The body is `files`, a list of message files, and optionally `gatherFailure`, an English sentence
saying why the files could not be kept. The answer is the message as the receiving side now holds it, with
`gathering` false. A message the receiving side does not hold is answered 404 `not-found`, and one that
promised no files 409 `conflict`.

### `POST /avatar-changes`

Word that kept profile pictures changed: taken for the first time, taken again, or dropped. The body is `via`,
`roomIds` and `personIds`, each a list of numbers whose picture changed. The word is for whatever shows the
pictures, which fetches them again through the provider's avatar routes. The answer is 201 with the word as
taken.

## The harness plugin

The protocol's harness plugin is `bmo-chat`: an MCP server and a skill, both in bmo-chat-plugin, which also
builds the `bmo-chat` command.

`bmo-chat mcp --provider <url>` starts the MCP server on standard input and output, driving the provider whose
routes stand at `<url>` through bmo-chat-client. The provider's token is given as `--token`, or in the
`BMO_CHAT_TOKEN` environment variable where the flag is left out. Each tool is one provider route, called
through the client:

| Tool | Route | Input |
|---|---|---|
| `chat_declaration` | `GET /declaration` | none |
| `chat_health` | `GET /health` | none |
| `chat_send` | `POST /sends` | `body`: a send |
| `chat_room` | `GET /rooms/{roomId}` | `roomId` |
| `chat_owner` | `GET /owner` | none |
| `chat_message_files` | `GET /rooms/{roomId}/messages/{messageId}/files` | `roomId`, `messageId`, optional `sentAt` |
| `chat_dialect_reading` | `POST /dialect/readings` | `body`: `text` |
| `chat_dialect_check` | `POST /dialect/checks` | `body`: `markup` |
| `chat_avatars` | `GET /avatars` | optional `after` and `limit` |

A tool answers with the route's answer as JSON, both as its structured content and as its text. A route
answered with an error, or a provider that could not be reached, is a tool error whose text is the error shape
the provider answered with, `status` beside it, or the sentence saying why nothing answered. The server offers
each request once, so a provider nothing listens at is answered at once with that sentence, and the harness
asks again when it chooses. The server sends no notifications: the provider routes publish no events.

The `bmo-chat` skill tells a harness building or testing a chat provider when and how to use the server:
read the declaration first, then send through the provider and check its dialect, all with no receiving side
running.

## Not in the protocol

| What | Where it belongs |
|---|---|
| a notice that a process beside the receiving side filed an answer in a room | the receiving side's own API for processes beside it; no provider calls it |
| per-platform send routes | `POST /sends`: the kind, not the path, carries the difference |
| speaking as the account's owner from a console, as text or as files | the receiving side and the console: it is their seam, not a provider's |
| news of anything but what a room said, and the event streams screens read | the receiving side and the services that publish them |
