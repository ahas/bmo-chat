# The chat wire protocol

Providers and the receiver are separate services and reach each other over HTTP with JSON bodies. Every route
below is part of the protocol. A route that is not here is not part of the base, whatever service answers it.
The pieces the bodies carry (messages, sends, refusals, declarations) are stated in `contract.md`.

There are two directions:

- **Provider routes**: a provider serves them (bmo-chat supplies the server) and the receiver calls them
  (bmo-chat-client supplies the client). One client reaches every provider at the base address the receiver's
  own deployment configuration gives for it.
- **Receiver routes**: the receiver serves them (bmo-chat-client supplies the server) and a provider calls them
  (bmo-chat supplies the link). The provider's own configuration gives the receiver's base address.

Paths below are relative to that base address. A service may mount the routes under a prefix of its own.

This document states version 0.5.0 of both libraries; a caller pins that version or a later one for every
route and field below.

## The OpenAPI document

`openapi.json`, beside this document, states every route below in OpenAPI 3.1 for a service written in any
language: each route's method and path, its request body, its answer and the error answers it can give, with
every body as a JSON Schema. A body is described as its sender writes it, so a field the sender may leave out
is not required, and a nullable field is required and may be null. Provider routes carry the tag `provider` and
receiver routes the tag `receiver`.

bmo-chat-client generates the document from the types it sends and reads, and its tests fail when the
committed file differs from what it generates. `cargo run -p bmo-chat-client --example openapi >
docs/openapi.json` writes it again. This document is the canonical statement: where the two disagree, the code
is repaired and the file generated again.

## Conventions

- **Bodies.** JSON, with field names in camelCase and enum values as `contract.md` writes them. An optional
  field may be left out. A field described as nullable is sent as null when it has no value. The one body that
  is not JSON is a profile picture, which is answered as its own bytes.
- **Bytes.** Bytes are base64 strings (standard alphabet, with padding) in the field that carries them.
- **Numbers.** Room, person and message numbers are signed 64-bit integers. Times are integers of epoch
  milliseconds.
- **Lists.** A route that answers a list takes one cursor, `after`, and `limit`: it answers only what comes
  after the cursor, in the order the route states, and at most `limit` entries. Either may be left out, for a
  list from its start or one with no bound.
- **Auth.** Every route, on both sides, needs an `Authorization: Bearer <token>` header. The token is supplied
  by the service that serves the routes, from its own configuration; the libraries read no environment
  variable themselves. A caller is configured with the token of the service it calls. A request with a
  missing or wrong token is answered 401.
- **Refusals are errors.** A send the provider does not perform and a file it cannot fetch are answered with a
  non-2xx status in the error shape below, like a request that could not be answered at all. A 2xx answer is
  always the route's success.

### The error shape

Every non-2xx answer has this body:

| Field | Type | Meaning |
|---|---|---|
| `code` | string | one of the codes below |
| `error` | string | an English sentence saying what went wrong |

| Status | `code` | When |
|---|---|---|
| 400 | `bad-request` | the body is not JSON, or the body or the query is not the shape the route takes; the sentence names what is missing |
| 401 | `unauthorized` | the token is missing or wrong |
| 404 | `no-route` | no route at this method and path |
| 404 | `nothing-held` | the route is right, but the service keeps nothing at the address the path names (a profile picture a provider has not kept); the sentence names the address |
| 409 | `not-ready` | the service is up but not ready to answer this route (a provider whose platform connection is not open yet) |
| 500 | `failed` | the service failed while answering; the sentence is the failure |

Codes that share a status are told apart by `code` alone: a wrong path from an empty address, and one refusal
from another.
`GET /avatars/{kind}/{id}` answers `nothing-held`, and so may a route a service serves of its own beside the
protocol's, which answers its errors in this same shape. bmo-chat's error shape answers as an HTTP response with
its code's status, so such a route returns it as it is. Both libraries carry `nothing-held` from version 0.2.0.

### Refusals

A refusal of `send` or `media` is answered in the error shape. Its `code` is the refusal's kind as `contract.md`
lists it, and `error` is the refusal's English sentence as the provider rendered it, which carries the
refusal's own fields (the capability, the agent, each missed picture with its reason). No other key crosses.

| Status | `code` | When |
|---|---|---|
| 403 | `speaker-not-allowed` | the send names a speaker, and the provider speaks as the room's own agent alone |
| 404 | `unavailable` | the platform would not give the file a `media` call asked for; the sentence says why |
| 409 | `not-ready` | the provider is not ready yet: the same code and status as the error above |
| 410 | `gone` | the message the call is about is one the platform no longer has, or will never let change again, so the caller lets go of it instead of asking again |
| 422 | `unsupported-capability` | the provider does not declare the capability the send or the call needs |
| 422 | `no-picture-sent` | none of the pictures of a send went |
| 422 | `speaker-unnumbered` | the provider has no number in the room for the agent named as speaker |
| 422 | `platform` | the platform refused, for the provider's own English reason |

### Retrying

A caller offers a request again only when the connection was refused, meaning nothing listened and the request
cannot have been received. It keeps offering within a patience window, which covers the time a service takes
to restart for a deployment. Two minutes, offered every two seconds, is the default. The window and the
interval can be set by the caller, so a test never waits on the real clock. A request that reached a listener
is never offered again, whatever it was answered. So a send is never made twice.

## Provider routes

`declaration`, `health`, `dialect/read`, `dialect/check` and the avatar routes answer whatever state the
provider is in. The other routes answer 409 `not-ready` while the provider's health is not ready.

### `GET /declaration`

The provider's self-declaration (`contract.md`, "The provider's declaration"). No body. The answer:

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

A mode's standing can change while the provider runs, so the receiver asks again instead of keeping an old
answer.

### `GET /health`

Whether the provider is ready. No body. The answer is `ready` (boolean) and `reason` (string or null: why not,
where it is not ready).

### `POST /send`

One send. The body is a send as `contract.md` states it: `verb`, `chatId`, the verb's own fields, and
optionally `as`, `speaker`, `tag` and `prefix`. Bytes (`bytes` on a document, `audio` on an audio or a voice,
`bytes` on an inline album item) are base64.

The answer, for a send the provider performed, is `messageId` (number or null) and optionally `missed`, a
list of the pictures of an album that did not go, each `address` and `reason`. A missed picture's `reason` is
an object with `code`, `message` (the English sentence) and, for `platform`, `detail`.

A send the provider does not perform is answered with a refusal (see Refusals). A send of a verb the provider
does not declare is refused `unsupported-capability` by the base before the provider is asked. A provider that
fails while sending answers 500 `failed`.

### `POST /media`

A picture or a file a message carried, fetched again from the platform. Gated by the `fetchMedia` capability.
The body is `chatId` and `messageId` (numbers), and optionally `sentAt` (number, epoch milliseconds), for a
platform that finds an old message by its time. The answer is a message file: `id`, `name`, `mime`, `bytes`
(base64) and optionally `kind`.

A file the platform would not give is answered 404 `unavailable`, and a call the provider refuses is answered
with its refusal (see Refusals). A provider that does not declare `fetchMedia` is refused
`unsupported-capability` by the base before the provider is asked.

### `POST /dialect/read`

The provider's dialect reading markdown. The body is `text` (the markdown). The answer:

| Field | Type |
|---|---|
| `markup` | string: what would go out |
| `plain` | string: the same words with the marks stripped |
| `formatted` | boolean |
| `parts` | list of strings: the markdown divided into pieces that each fit one message |
| `fold` | string or null: the folded markup, or null where the dialect does not fold or there is nothing worth folding |

### `POST /dialect/check`

The provider's dialect checking markup it built. The body is `markup`. The answer is `fault` (string or null:
null when the markup is sound, else the reason) and `visible` (string: what a reader sees of it).

### Room routes

Each takes `chatId` (number) as its body, except `owner-room`, which takes none.

| Route | Answer |
|---|---|
| `POST /room/owns` | `owns`: boolean |
| `POST /room/identity` | `identity`: number or null, the sender number the agent's own words carry in the room |
| `POST /room/is-owner` | `kind`: `owner` or `group` |
| `POST /room/private-of` | `person`: number or null, the one person in a one-to-one with the agent |
| `POST /room/title` | `title`: string or null |
| `GET /owner-room` | `chatId`: number or null, the room to reach the owner in unprompted |

### `GET /avatars`

The profile pictures the provider keeps (`contract.md`, "Profile pictures"). No body; the query takes `after`
(a time) and `limit`. The answer is `avatars`, a list of the pictures, each `kind` (`room` or `person`), `id`
(number) and `takenAt` (number: when the picture was taken), oldest taken first and only those taken after
`after`. A list never ends between two pictures taken at the same moment, so it holds more than `limit`
entries where `limit` would part them. A provider that does not declare `avatars` answers an empty list.

### `GET /avatars/{kind}/{id}`

The picture the provider keeps for a room (`kind` `room`) or a person (`kind` `person`) by its number. No body.
The answer is the picture's bytes, with its type as `Content-Type`. Where none is kept, or the provider does not
declare `avatars`, the answer is 404 `nothing-held`; where `kind` is neither or `id` is not a number, 404
`no-route`.

## Receiver routes

A hand-over answers 200 once the receiver has taken it, with an empty JSON object unless the route states an
answer of its own; a question answers 200 with its answer. A call the receiver could not take or answer is
answered with the error shape.

### `POST /message`, `POST /attachment`, `POST /edited`, `POST /history`

A message a room said, a message carrying a file, a message already handed over and changed, and a past message
read back from a room. All four take the same body:

| Field | Type |
|---|---|
| `message` | a message: `chatId`, `messageId`, `senderId` (number or null), `senderName`, `sentAt`, `text`, `replyToMessageId` (number or null), `via`, `album` (number or null), `mode` |
| `context` | optional context: `roomTitle`, `senderUsername`, `replyToSenderId`, `spokenText`, `metadata` (any JSON), `carriesFile`, `gathering`, `tag`, each optional |
| `room` | optional room facts: `kind` (`owner` or `group`), `identity` (number or null), `privateTo` (number or null), each optional |
| `files` | optional list of message files, each `id`, `name`, `mime`, `bytes` (base64) and optionally `kind`: the files an `attachment` carries when they are already gathered |

An `attachment` whose file is still being gathered says `gathering: true` in its context, carries no `files`,
and is followed by a `gathered` hand-over for the same message.

An `edited` message replaces the one the receiver holds under its `chatId` and `messageId`: the receiver keeps
its words and its sender (`senderId`, `senderName`) as the hand-over carries them.

A `history` message is archived where the receiver does not hold it yet, and wakes nobody and routes nothing.
Its answer is `kept` (boolean): true where the receiver archived it, false where it already held the message.

A message file's `id` is a string, required, the same each time the same file crosses and never shared by two
files of one message (`contract.md`, "Files"). An inline album item of a send is a file item and carries no
`id`. Both libraries carry the `id` from version 0.3.0.

### `POST /gathered`

The file a message's `gathering` promised. The body is `chatId`, `messageId` and `via`, and `outcome`:

| `outcome` | Other fields |
|---|---|
| `kept` | `files`, a list of message files |
| `not-kept` | `reason`: an English sentence saying why the file could not be kept |

### `POST /avatars/updated`

Word that kept profile pictures changed: taken for the first time, taken again, or dropped. The body is `via`,
`rooms` and `people`, each a list of numbers whose picture changed. The word is for whatever shows the
pictures, which fetches them again through the provider's avatar routes.

### `POST /rooms/listened`

Whether the receiver listens to a room. The body is `chatId` and `via`; the answer is `listened` (boolean).

### `POST /rooms/of`

The provider's rooms whose past it may read back, each with the oldest message the receiver holds of it. The
body is `via`, and optionally `after` (a room number) and `limit`. The answer is `rooms`, a list in ascending
order of `chatId` and only those after `after`, each `chatId` and `oldestMessageId` and `oldestSentAt` (each a
number, or null where the receiver holds no message of the room).

## Routes not in the protocol

These existed between the chat services and the receiver before. They are not part of the base:

| Route | Where it went |
|---|---|
| a hand-over of a message already archived | merged into `message`: the receiver archives what it is handed, whatever its origin |
| a notice that a process beside the receiver filed an answer in a room, by ids only | the receiver's own API for processes beside it; no provider calls it |
| per-platform send routes (one for each platform the receiver used to carry itself) | `send`: the verb, not the path, carries the difference |
| sending in the owner's own words, as text or as an album of files | `send` with `as` naming the owner's voice; an album takes inline file items |
| fetching a picture or a file through a reader account | `media` |
| asking who the agent is in a room | `room/identity` |
| health | kept, as `health` |
| the web console speaking as the owner into any room, as text or as files | the receiver and the web service: it is their seam, not a provider's |
| news from scripts (an item changed, a worker wrote a record) | the receiver and the web service |
| the event stream and the live snapshot screens read | the receiver and the web service |
