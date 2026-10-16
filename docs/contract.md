# The chat contract

This repository holds the two base libraries every chat service and the chat receiver build on.

- **bmo-chat** is what a chat service imports. A chat service (a provider) holds one platform's connection,
  declares what it is and what it can do, performs the send verbs, and hands what its rooms say to the
  receiver.
- **bmo-chat-client** is what the receiver imports. With it the receiver reaches every provider the same way,
  and takes in what every provider hands over the same way. There is no per-provider implementation on the
  receiver's side.

The two libraries talk only over HTTP, in the shapes `protocol.md` states. Neither depends on the other, and
neither depends on any other `bmo-` package: each defines the same wire shapes on its own, and a test in the
repository root checks that the two still agree.

This document states the pieces: what a message, a room, a sender, a capability and a send are. `protocol.md`
states the routes that carry them. Field names below are as they cross the wire.

## What the base holds, and what it does not

The base holds chat piece types, the provider's self-declaration, the send verbs and their results, and the
wire protocol. It holds nothing of turn templates or their assembly, and no job, memory, agent, gate or archive
concept. No signature in either library takes a database, a receiver's context, or an output gate.

The base holds no list of providers and never branches on a provider's key. A provider's key may be stored on
a row, written to a log, shown on a screen, and used by the provider to find its own configuration. Nothing
compares it to a literal. Every difference the receiver needs is asked of what the provider declares: its
modes, its capabilities, its dialect, its numbering, and its answers about a room. The receiver learns which
providers exist from its own deployment configuration, as a list of provider base addresses, and asks each one
for its declaration.

Each provider reads its own environment for its own configuration (its platform credentials, its port, the
owner's account on its platform). The base passes nothing to a provider when it starts except the link it
uses to reach the receiver.

## Identifiers

Rooms, people and messages are identified by number. An agent that speaks a send is identified by its
registry id, an opaque string. No human-readable name of an agent crosses the wire; whatever shows a name to a
person resolves it from the id. The display name a platform gives a person who wrote a message is message data
and crosses as `senderName`.

### Numbering

`ChatNumberSpace` says whose numbers a provider's rooms, people or messages carry:

| Value | Meaning |
|---|---|
| `native` | the platform's own numbers, used as they are |
| `assigned` | numbers the provider gave out because the platform's cannot be used |
| `none` | none of its own: the provider speaks into rooms and people another provider numbers |

`ChatNumbering` declares one number space for each of `rooms`, `people` and `messages`. How a provider assigns
numbers, and where it keeps them, is the provider's own.

## Read modes

A read mode is one way a provider's rooms are read. It is two axes and a standing.

`ChatVoice`, whose account speaks:

| Value | Meaning |
|---|---|
| `bot-account` | the platform's official bot interface |
| `human-reader` | a person's account, read only; a send in the owner's own words goes out in it |
| `human-avatar` | a person's account, with the agent answering in it behind a prefix the caller supplies |
| `human-as-agent` | an account that belongs to the agent itself |
| `direct` | no platform: the owner typing into the machine |

`ChatReach`, what that account receives: `all-messages` or `mentions-only`. It is a property of the mode on
that platform, not of the platform.

`ChatModeStanding`, where the mode stands:

| Value | Meaning |
|---|---|
| `active` | running |
| `inactive` | supported, waiting on something it is missing |
| `unsupported` | refused by the platform's own terms; no configuration makes it active |

`ChatReadMode` carries `voice`, `reach`, `standing`, a `reason` (what is missing for an inactive mode, or the
platform's own words for an unsupported one; null for an active one) and `canReplyTo` (whether an answer can
be pinned to a message this mode delivered). A provider may have several modes active at once. A mode that is
not active is never started.

`modeOf` finds a provider's mode by voice. It is how the mode a message came in on is found, and with it
whether an answer to that message can be pinned to it.

## Capabilities

`ChatVerb` names the twelve send verbs: `text`, `photo`, `album`, `document`, `audio`, `voice`, `sticker`,
`reaction`, `pin`, `typing`, `edit`, `delete`.

`ChatCapability` is the twelve verbs plus four answers that are not verbs:

| Capability | Meaning |
|---|---|
| `replyTo` | an answer can be aimed at a particular message |
| `fetchMedia` | a picture or a file can still be fetched after its message arrived |
| `avatars` | the provider supplies profile pictures |
| `history` | the provider can read a room's past messages back |

The full capability set is all sixteen, taken from the type itself, so a reader handed a capability's name
checks it against that set and keeps no list of its own. A provider declares the capabilities it has as `can`.
A send of a verb it does not declare is refused (see Refusals).

## Dialects

A dialect is what the agent's markdown becomes on a provider's rooms. One dialect may serve several providers,
so a dialect names no provider. A dialect has:

| Member | What it is |
|---|---|
| `name` | the dialect's name |
| `limits` | `ChatDialectLimits`: `text` (the longest message), `caption` (the longest caption), `albumPhotos` (the most pictures in one album), `fileBytes` (the largest file); each null where the platform sets no limit |
| `writing` | the sentence telling a writer in such a room what its marks become |
| `foldable` | whether this dialect can fold a long answer; a dialect that cannot is never asked for a fold |
| `convert` | markdown in; `ChatMarkup` out: `markup` (what goes out), `plain` (the same words with the marks stripped), `formatted` (whether the markup says anything the plain text does not) |
| `parts` | markdown in; the pieces each of which fits one message; a dialect that does not split answers with the one piece it was given |
| `fault` | markup this dialect built in; null when it is sound, else the reason |
| `fold` | markdown in; the folded markup, or null when there is nothing worth folding |
| `visible` | markup in; what a reader actually sees of it |

Whether to fold an answer is decided upstream of the provider. A text send carries that decision as `fold`, and
the provider applies its dialect's fold where its dialect is foldable.

The dialect runs in the provider. The receiver reaches it through the dialect routes in `protocol.md`.

The base supplies one dialect as the safe default, `plain`, for a platform whose rooms take words and nothing
else. It has no limits, does not fold, does not split, never faults, and shows its markup as it stands. Its
conversion strips every mark, so markup and plain text are the same words and `formatted` is false. Its
`writing` is: "This room shows no formatting. Write markdown as ever, but every mark is stripped before the
words go out, so nothing may rest on a mark to be understood: say it in the words themselves."

## The provider's declaration

A provider declares itself and the base holds no record of it anywhere else:

| Member | What it is |
|---|---|
| `key` | short, ASCII, stable; written onto every message the provider hands over |
| `label` | what the platform is called where a person reads it |
| `configured` | null when the provider can run, else the reason it cannot |
| `modes` | every mode it supports, each with its standing |
| `can` | its capabilities |
| `dialect` | its dialect's `name`, `limits`, `writing` and `foldable` |
| `shows` | what a person reading this provider's rooms sees beyond what the provider sends; null where it shows only what it carries |
| `numbering` | its `ChatNumbering` |
| `speakers` | whether a send may name an agent speaker other than the one the room is answered by |

### Questions about a room

A provider answers these about any room number, each through its own route:

| Question | Answer |
|---|---|
| owns room | whether this room is one of this provider's |
| identity | the sender number the agent's own words carry in this room, or null |
| room kind | `RoomKind`: `owner` when the room is the owner's private chat on this platform, else `group` |
| private room of | the sender number of the one person in a one-to-one with the agent, or null for a room of several, a room answered in the avatar voice, or one the platform has not said is a one-to-one |
| title | what the platform calls the room, or null where it cannot or will not say |
| owner room | (not about a given room) the room to speak into to reach the owner unprompted, or null |

Whether a person behind a private room is a user is the receiver's question, not the provider's.

## Messages

`ChatMessage` is a message as it crosses from a provider to the receiver. Nothing platform-shaped crosses:

| Field | Type | Meaning |
|---|---|---|
| `chatId` | number | the room |
| `messageId` | number | the message |
| `senderId` | number or null | who said it |
| `senderName` | string | the name the platform shows for who said it |
| `sentAt` | number | when, in Unix seconds |
| `text` | string | its words, or a placeholder the provider wrote for what was sent |
| `replyToMessageId` | number or null | the message it answers |
| `via` | string | the key of the provider that took it in |
| `album` | number or null | the number of the first message of the send of several this one came in |
| `mode` | `ChatVoice` | the mode that delivered it |

A message carries no file path. The files a message carries cross the wire as bytes (see Files).

### Context

`InboundChatContext` is what the platform said about a message beyond the message itself. Every field is
optional:

| Field | Meaning |
|---|---|
| `roomTitle` | what the platform calls the room, where it says so with the message |
| `senderUsername` | the handle of whoever said it, where the platform has handles |
| `replyToSenderId` | who wrote the message this one answers |
| `spokenText` | the words the person actually typed, where `text` carries a placeholder (the caption under a file, for example) |
| `metadata` | the platform's own payload, opaque; nothing outside the provider reads into it |
| `carriesFile` | whether the message carried a file |
| `gathering` | the provider is still gathering the message's file and will hand it over later as `gathered`; whatever is woken for the file waits for it |
| `tag` | the opaque tag of the send this message is the platform's echo of, where the send gave one |

### Room facts

A hand-over may carry what the provider already knows of the room, so the receiver need not ask for it:
`kind` (`RoomKind`), `identity` and `privateTo`, meaning the same as the room questions above. Each is
optional, and the receiver can always ask the room routes instead.

### Files

A file crosses as a file item: `name`, `mime` and `bytes`. A provider gathers a file from its platform. Where
it is kept is the receiver's concern, so the provider hands the bytes over:

- when the file is already gathered, as the files of the `attachment` hand-over;
- when it is still on its way, `attachment` says `gathering` and a `gathered` hand-over follows. Its outcome
  is either `kept` with the files, or `not-kept` with the reason.

## The receiver, as a provider sees it

`ChatReceiver` is the receiver's side as a provider calls it, and a provider asks nothing else of the
receiver:

| Call | What it hands over |
|---|---|
| message | a message a room said |
| attachment | a message carrying a file, with the file or the promise of it |
| edited | a message already handed over, said again with different words |
| gathered | the file a message's `gathering` promised, or why it was not kept |

The receiver archives what it is handed whatever its origin. A provider never hands over a message as already
archived.

## Sends

A send is one verb at a time. Every send carries `chatId` and its `verb`, and may carry:

- `as`: the voice to speak with, for a provider that reads by more than one (a send in the owner's own words
  names `human-reader`). Unnamed, the provider speaks with the voice it answers the room by. A voice that is
  not active is refused, with the reason.
- `speaker`: `{ agent }`, the registry id of an agent other than the one the room is answered by. Only a
  provider that declares `speakers` takes it.
- `tag`: an opaque string the provider returns on the inbound message that is the platform's echo of this
  send. It is meant for a provider whose platform gives no message number back for a send.

| Verb | Fields beyond `chatId` |
|---|---|
| `text` | `text`; optional `replyTo`, `silent` (a notice that must not buzz a phone), `fold` (fold where the dialect can) |
| `photo` | `imageUrl`; optional `caption`, `replyTo` |
| `album` | `items`, each either `{ address }` (a web address or a file path the provider can read) or an inline file item `{ name, mime, bytes }`; optional `caption`, `replyTo` |
| `document` | `bytes`, `fileName`; optional `replyTo` |
| `audio` | `audio`, `fileName`; optional `replyTo` |
| `voice` | `audio`, `said` (what the line said); optional `caption`, `transcript` (what was actually read out), `replyTo` |
| `sticker` | `emoji` |
| `reaction` | `messageId`, `emoji`; an empty emoji takes the agent's own reaction off |
| `pin` | `messageId`, `on`; optional `silent` |
| `typing` | `on` |
| `edit` | `messageId`, `text` |
| `delete` | `messageId` |

`replyTo` is a modifier, not a verb. A provider that does not declare `replyTo` ignores it.

Sanitising the words and deciding whether to fold them happen upstream. A provider takes the words as they
come and applies only its dialect. Keeping a copy of what went out is the sender's own concern: the sender
holds the bytes it sent, and the send's result gives it the message number to keep them under.

### Results

`ChatSendResult` is one of:

- **sent**: `messageId` (null where the platform hands none back), and optionally `missed`, the pictures
  of an album that did not go, each a `MissedPicture`.
- **refused**: a `refusal` (see below), and `gone` (true when the refusal is about a message the
  platform no longer has, or will never let change again, so the caller lets go of it instead of asking
  again).

A send that the provider does not perform is answered as refused and never dropped. A provider that fails
while performing a send answers with the error shape in `protocol.md`.

`MissedPicture` is a picture by the `address` it was given as, with a `reason`.

## Refusals

Every refusal is a fixed English sentence and a code a caller can match on. Translation into a person's
language happens where the language packs are, never here. The sentence takes `label`, the provider's label,
and the variant's own fields.

`ChatRefusal`:

| Code | Fields | Sentence |
|---|---|---|
| `unsupported-capability` | `label`, `capability` | "{label} cannot {what}." where {what} is the capability's words below |
| `no-picture-sent` | `label`, `missed` | "{label}: no picture was sent." followed by one line per missed picture, "{address}: {reason}" |
| `speaker-not-allowed` | `label`, `agent` | "{label} cannot send in the name of agent {agent}." |
| `speaker-unnumbered` | `label`, `agent` | "{label} has no number for agent {agent}, so it cannot send in that agent's name." |
| `not-ready` | `label` | "{label} is not ready yet." |
| `platform` | `label`, `detail` | "{label}: {detail}" where {detail} is the provider's own English reason |

The capability words:

| Capability | Words |
|---|---|
| `text` | send a message |
| `photo` | send a photo |
| `album` | send an album of photos |
| `document` | send a file |
| `audio` | send an audio file |
| `voice` | send a voice message |
| `sticker` | send a sticker |
| `reaction` | react to a message |
| `pin` | pin a message |
| `typing` | show a typing indicator |
| `edit` | edit a sent message |
| `delete` | delete a sent message |
| `replyTo` | reply to a particular message |
| `fetchMedia` | fetch a picture or a file later |
| `avatars` | fetch profile pictures |
| `history` | read past messages back |

`MissedPictureReason`, the `reason` of a missed picture:

| Code | Sentence |
|---|---|
| `not-an-address` | "It is neither a web address nor a file path on this machine." |
| `outside-data-root` | "The file is outside the data folder, so it was not sent." |
| `missing-file` | "There is no such file on this machine." |
| `unfetched` | "The picture could not be fetched." |
| `platform` | the provider's own English reason, carried as `detail` |

A file a provider could not keep crosses as the `not-kept` outcome of `gathered`, with the reason in English.

## What is deliberately not here

| What | Where it belongs |
|---|---|
| the announcement a room hears when it is shut | the receiver |
| refusal and placeholder text that belongs to one platform | that platform's provider repository, in English |
| compositing pictures (laying a transparent image on a plain ground, for example) | whichever provider repository needs it |
| numbering algorithms and the tables that hold assigned numbers | the provider that assigns them |
| archiving, routing, waking, room rules, permissions and windows | the receiver |
| output gates, sanitising, fold decisions and turn assembly | the gate and the agent services |
| the names and personas of agents | the agent registry |
| the language packs | the web service |
