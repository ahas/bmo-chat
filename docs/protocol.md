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

## Conventions

- **Bodies.** JSON, with field names in camelCase and enum values as `contract.md` writes them. An optional
  field may be left out. A field described as nullable is sent as null when it has no value.
- **Bytes.** Bytes are base64 strings (standard alphabet, with padding) in the field that carries them.
- **Numbers.** Room, person and message numbers are signed 64-bit integers. Times are Unix seconds.
- **Auth.** Every route, on both sides, needs an `Authorization: Bearer <token>` header. The token is supplied
  by the service that serves the routes, from its own configuration; the libraries read no environment
  variable themselves. A caller is configured with the token of the service it calls. A request with a
  missing or wrong token is answered 401.
- **Refusals are answers.** A send the provider does not perform, a picture it cannot fetch and the like are
  answered 200 with a typed refusal in the body. The error shape below is only for a request that could not be
  answered at all.

### The error shape

Every non-2xx answer has this body:

| Field | Type | Meaning |
|---|---|---|
| `code` | string | one of the codes below |
| `error` | string | an English sentence saying what went wrong |

| Status | `code` | When |
|---|---|---|
| 400 | `bad-request` | the body is not JSON, or not the shape the route takes; the sentence names what is missing |
| 401 | `unauthorized` | the token is missing or wrong |
| 404 | `no-route` | no route at this method and path |
| 409 | `not-ready` | the service is up but not ready to answer this route (a provider whose platform connection is not open yet) |
| 500 | `failed` | the service failed while answering; the sentence is the failure |

### Retrying

A caller offers a request again only when the connection was refused, meaning nothing listened and the request
cannot have been received. It keeps offering within a patience window, which covers the time a service takes
to restart for a deployment. Two minutes, offered every two seconds, is the default. The window and the
interval can be set by the caller, so a test never waits on the real clock. A request that reached a listener
is never offered again, whatever it was answered. So a send is never made twice.

## Provider routes

`declaration`, `health`, `dialect/read` and `dialect/check` answer whatever state the provider is in. The
other routes answer 409 `not-ready` while the provider's health is not ready.

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
| `numbering` | `rooms`, `people`, `messages`, each a number space |
| `speakers` | boolean |

A mode's standing can change while the provider runs, so the receiver asks again instead of keeping an old
answer.

### `GET /health`

Whether the provider is ready. No body. The answer is `ready` (boolean) and `reason` (string or null: why not,
where it is not ready).

### `POST /send`

One send. The body is a send as `contract.md` states it: `verb`, `chatId`, the verb's own fields, and
optionally `as`, `speaker` and `tag`. Bytes (`bytes` on a document, `audio` on an audio or a voice, `bytes` on
an inline album item) are base64.

The answer is a send result, distinguished by `sent`:

| `sent` | Other fields |
|---|---|
| true | `messageId` (number or null); optional `missed`, a list of missed pictures, each `address` and `reason` |
| false | `refusal`, a refusal object; `gone` (boolean) |

A refusal object is `code`, `message` (the English sentence as the provider rendered it) and the variant's own
fields as `contract.md` lists them: `label` on every refusal, plus `capability`, `missed`, `agent` or `detail`.
A missed picture's `reason` is an object with `code`, `message` and, for `platform`, `detail`.

A send of a verb the provider does not declare is answered `sent: false` with an `unsupported-capability`
refusal, by the base before the provider is asked. A provider that fails while sending answers 500 `failed`.

### `POST /media`

A picture or a file a message carried, fetched again from the platform. Gated by the `fetchMedia` capability.
The body is `chatId` and `messageId` (numbers), and optionally `sentAt` (number), for a platform that finds an
old message by its time. The answer is distinguished by `outcome`:

| `outcome` | Other fields |
|---|---|
| `fetched` | `name`, `mime`, `bytes` (base64) |
| `unavailable` | `reason`: an English sentence saying why the platform would not give it |
| `refused` | `refusal`, a refusal object (`unsupported-capability` for a provider that does not declare `fetchMedia`) |

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

## Receiver routes

Each hand-over answers 200 with an empty JSON object once the receiver has taken it. A hand-over the receiver
could not take is answered with the error shape.

### `POST /message`, `POST /attachment`, `POST /edited`

A message a room said, a message carrying a file, and a message said again with different words. All three
take the same body:

| Field | Type |
|---|---|
| `message` | a message: `chatId`, `messageId`, `senderId` (number or null), `senderName`, `sentAt`, `text`, `replyToMessageId` (number or null), `via`, `album` (number or null), `mode` |
| `context` | optional context: `roomTitle`, `senderUsername`, `replyToSenderId`, `spokenText`, `metadata` (any JSON), `carriesFile`, `gathering`, `tag`, each optional |
| `room` | optional room facts: `kind` (`owner` or `group`), `identity` (number or null), `privateTo` (number or null), each optional |
| `files` | optional list of file items, each `name`, `mime`, `bytes` (base64): the files an `attachment` carries when they are already gathered |

An `attachment` whose file is still being gathered says `gathering: true` in its context, carries no `files`,
and is followed by a `gathered` hand-over for the same message.

### `POST /gathered`

The file a message's `gathering` promised. The body is `chatId`, `messageId` and `via`, and `outcome`:

| `outcome` | Other fields |
|---|---|
| `kept` | `files`, a list of file items |
| `not-kept` | `reason`: an English sentence saying why the file could not be kept |

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
