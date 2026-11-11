# The chat contract

This repository holds the chat protocol: two base libraries every chat service and every receiving side build
on, and the protocol's harness plugin.

- **bmo-chat** is what a chat service imports. A chat service (a provider) holds one platform's connection,
  declares what it is and what it can do, performs the sends, and hands what its rooms say to the receiving
  side.
- **bmo-chat-client** is what the receiving side imports: whatever unit takes in what providers hand over. With
  it the receiving side reaches every provider the same way, and takes in what every provider hands over the
  same way. There is no per-provider implementation on the receiving side.
- **bmo-chat-plugin** is the harness plugin, `bmo-chat`: the `bmo-chat` command, whose MCP server drives any
  provider at a given address through bmo-chat-client, and the skill that tells a harness how to use it
  (`protocol.md`, "The harness plugin"). It also generates the OpenAPI documents.

The two libraries talk only over HTTP, in the shapes `protocol.md` states. Neither depends on the other, and
neither depends on any other `bmo-` package: each defines the same wire shapes on its own, and a test in the
repository root checks that the two still agree. The plugin depends on bmo-chat-client alone.

This document states the pieces: what a message, a room, a sender, a capability and a send are. `protocol.md`
states the routes that carry them. Field names below are as they cross the wire.

## What the base holds, and what it does not

The base holds chat piece types, the provider's self-declaration, the sends and their results, and the wire
protocol. It holds nothing beyond chat: nothing of who composes the words a send carries or how, and nothing
of what the receiving side does with what it is handed. No signature in either library takes a database, a
receiving side's context, or a check on outgoing words.

The base holds no list of providers and never branches on a provider's key. A provider's key may be stored on
a row, written to a log, shown on a screen, and used by the provider to find its own configuration. Nothing
compares it to a literal. Every difference the receiving side needs is asked of what the provider declares:
its modes, its capabilities, its dialect, its numbering, and its answers about a room. The receiving side
learns which providers exist from its own configuration, as a list of provider base addresses, and asks each
one for its declaration.

Each provider reads its own environment for its own configuration (its platform credentials, its port, the
owner's account on its platform). The base passes nothing to a provider when it starts except the link it
uses to reach the receiving side.

## Identifiers

Rooms, people and messages are identified by number. A speaker a send names is the caller's opaque string,
which the base never reads; whatever shows a name to a person resolves it. The display name a platform gives a
person who wrote a message is message data and crosses as `senderName`.

### Numbering

`ChatNumberSpace` says whose numbers a provider's rooms, people or messages carry:

| Value | Meaning |
|---|---|
| `native` | the platform's own numbers, used as they are |
| `assigned` | numbers the provider gave out because the platform's cannot be used |
| `none` | none of its own: the provider speaks into rooms and people another provider numbers |

`ChatNumbering` declares one number space for each of `rooms`, `people` and `messages`, and `ranges`: for
`rooms` and for `people`, the `ChatNumberRange` every number the provider assigns in that space falls in, as
`first` and `last`, both included. A space numbered `assigned` carries its range, and a space numbered any other
way carries null. Messages carry no range: a message is numbered within its room, so two providers' messages
never meet where their rooms do not.

Two providers' ranges of the same space never overlap, so no number one provider gives out is ever another's.
bmo-chat-client checks a numbering on its own (an `assigned` space without a range, a range for a space that
is not `assigned`, a range whose `first` is above its `last`) and against another provider's (a range of the
same space that overlaps). The receiving side takes in no provider whose numbering fails either check. The
protocol is told no platform's native numbers, so keeping a range clear of them is the provider's own choice
of range.

How a provider assigns numbers within its range, and where it keeps them, is the provider's own.

## Read modes

A read mode is one way a provider's rooms are read. It is two axes and a standing.

`ChatVoice`, whose account speaks:

| Value | Meaning |
|---|---|
| `bot-account` | the platform's official bot interface |
| `human-reader` | a person's account, read only; a send in the owner's own words goes out in it |
| `human-avatar` | a person's account, answered in behind a prefix the caller supplies |
| `own-account` | an ordinary account of the provider's own, held by no person |
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

Whether a provider can answer as the person whose account it reads is its `human-avatar` mode, and nothing
else says it: the mode standing `active` is a provider that sends in that person's account now. A platform
whose terms forbid automating a person's account declares the mode `unsupported`, with its terms' words as the
reason, so a provider checks its platform's terms before it declares the mode any other way.

`modeOf` finds a provider's mode by voice. It is how the mode a message came in on is found, and with it
whether an answer to that message can be pinned to it.

## Capabilities

`ChatVerb` names the twelve kinds of send: `text`, `photo`, `album`, `document`, `audio`, `voice`, `sticker`,
`reaction`, `pin`, `typing`, `edit`, `delete`.

`ChatCapability` is the twelve kinds plus eight answers that are not sends:

| Capability | Meaning |
|---|---|
| `replyTo` | an answer can be aimed at a particular message |
| `fetchMedia` | a picture or a file can still be fetched after its message arrived |
| `avatars` | the provider supplies profile pictures |
| `history` | the provider can read a room's past messages back |
| `roomList` | the provider can list the rooms its account is in |
| `contactList` | the provider can list the contacts of its account |
| `startChat` | the provider can start a one-to-one with a contact of its account |
| `startGroup` | the provider can start a room of several contacts of its account, under a title |

The full capability set is all twenty, taken from the type itself, so a reader handed a capability's name
checks it against that set and keeps no list of its own. A provider declares the capabilities it has as `can`.
A send of a kind it does not declare is refused (see Refusals).

### Profile pictures

A provider that declares `avatars` keeps profile pictures of rooms and people and serves them through the
avatar routes in `protocol.md`. A kept picture is known by its subject, `ChatAvatarKind` (`room` or `person`)
and the subject's number, and is listed as a `ChatAvatar`: `kind`, `id` and `takenAt`, the moment the provider
took the picture it keeps now. The picture itself is a `ChatPicture`: its `mime` type and its bytes.

Where a provider keeps no picture for the subject asked, the picture route answers 404 `not-found`, never
`no-route`, so a missing picture is told apart from a wrong path. A provider that does not declare `avatars`
keeps none: the base answers its list as empty and every picture as `not-found` without asking it. Which
pictures a provider takes, how often, and where it keeps them is the provider's own; it tells the receiving
side which subjects' pictures changed (see "The receiving side, as a provider sees it").

## Dialects

A dialect is what markdown written for a provider's rooms becomes on them. One dialect may serve several
providers, so a dialect names no provider. A dialect has:

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

Whether to fold an answer is decided before the send reaches the provider. A text send carries that decision
as `fold`, and the provider applies its dialect's fold where its dialect is foldable. The base supplies that
wiring: the markup a text goes out as is the dialect's fold when the send asks for one, the dialect is
foldable and the fold is not null, and its conversion's `markup` otherwise.

The dialect runs in the provider. A caller reaches it through the dialect routes in `protocol.md`.

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
| `speakers` | whether a send may name a speaker other than the one the room is answered by |

### Health

A provider says whether it is ready to answer about rooms and to send: `ready`, and where it is not, the
`reason` in English, with its own `version`. A provider that says nothing of its own is ready exactly when
`configured` is null, and otherwise not ready for the reason `configured` gives. While a provider is not ready,
the base answers its send, room, owner and file routes as not ready without asking it (see `protocol.md`).

### A room as a provider knows it

A provider answers what it knows of any room number as one `ProviderRoom`:

| Member | What it is |
|---|---|
| `id` | the room |
| `owns` | whether this room is one of this provider's |
| `identity` | the sender number the account's own words carry in this room, or null |
| `kind` | `RoomKind`: `owner` when the room is the owner's private chat on this platform, else `group` |
| `privateTo` | the sender number of the one person in a one-to-one with the account, or null for a room of several, a room answered in the avatar voice, or one the platform has not said is a one-to-one |
| `title` | what the platform calls the room, or null where it cannot or will not say |
| `modes` | the voices the room is read by and may be sent in, each one of the declaration's modes; null where the provider does not say, which stands for every mode the declaration lists |

A room's `modes` are the provider's own account of the room: a provider whose rooms all take the same modes
lists its declared ones for every room, and one whose platform reads some rooms through another account lists
each room's own. A send naming a voice the room's `modes` do not hold, or naming none in a room whose `modes` do
not hold the voice the provider answers rooms by, is refused `platform`, with the provider's own reason.

A room whose `modes` hold `human-reader` alone is **read-only**: it is read through a person's account, and only
that person's own words go into it. Nothing answers there, so the receiving side wakes no answerer in it and
offers it no mode that answers. Whether the people's words in such a room are read at all is the receiving
side's to ask for (see "The receiving side, as a provider sees it"); `identity` keeps its meaning there, and is
null where no words of the provider's own account stand in the room.

A provider that knows nothing of a room answers none, and the route answers `not-found`. Apart from any room,
it answers the room to speak into to reach the owner unprompted, or null. Whether a person behind a private
room is a user is the receiving side's question, not the provider's.

### The rooms of the account

A provider that declares `roomList` lists the rooms its account is in, each as the `ProviderRoom` it answers
for that room on its own, so a person can choose among them. Which of its accounts it lists, and how it learns
their rooms, is the provider's own. The base pages the list in ascending order of room number and asks only a
provider that declares `roomList`; one that does not is refused `unsupported-capability` without being asked.

A listed room is the provider's account's alone, and says nothing of the receiving side: no `ProviderRoom`
carries whether the receiving side holds the room, and no provider is asked it. A receiving side that shows the
listing to a person marks each room it already holds itself, from its own record and beside the provider's
answer, by the room's number, which is the same number on both sides.

### The contacts of the account

A provider that declares `contactList` lists the contacts of its account, the people it could start a
conversation with, so a person can choose among them. Which of its accounts it lists, and what its platform
counts as a contact, is the provider's own. Each is a `ChatContact`:

| Member | What it is |
|---|---|
| `id` | the contact, as a number of the provider's `people` space |
| `name` | what the account calls the contact |
| `kind` | `ChatContactKind`: `person` for a person's account, `bot` for a platform's bot account |
| `roomId` | the one-to-one the account already has with the contact, as a number of the provider's `rooms` space, or null where it has none |

The base pages the list in ascending order of contact number and asks only a provider that declares
`contactList`; one that does not is refused `unsupported-capability` without being asked. As with a listed room,
whether the receiving side holds a contact's `roomId` is the receiving side's to mark, never the provider's.

### Starting a conversation

A provider that declares `startChat` or `startGroup` starts a conversation on its platform when asked. A
`ChatStart` is one of two shapes:

| Shape | Members | Capability |
|---|---|---|
| one-to-one | `contactId`: one contact of the account | `startChat` |
| group | `title`: the room's name, not blank; `contactIds`: the contacts to bring in, at least one, each once | `startGroup` |

A start comes to one of:

- **started**: the room, as the `ProviderRoom` the provider answers for it from then on. A one-to-one with a
  contact the account already has one with comes to that room, and nothing new is made. The room's `modes`
  say whose account it stands in, as any room's do: a provider that can start a one-to-one only through a
  person's account answers it read-only where its own account is not in it.
- **no contact**: a contact the start names is none the provider knows, answered `not-found`.
- **refused**: a `ChatRefusal`. A shape whose capability the provider does not declare is refused
  `unsupported-capability` by the base before the provider is asked; what the platform will not do is refused
  `platform` with the provider's own reason.

A group naming no contact, naming one twice or carrying a blank title is no start of either shape, and the base
answers it `bad-request` without asking the provider.

Starting a conversation sends nothing into it. Where a platform makes a one-to-one stand only once a message is
in it, the provider answers the room the first message will make, under the number it will carry.

### Contacts and starts on the platforms that do not declare them

What a provider of each platform below needs before it can declare `contactList`, `startChat` or `startGroup`;
until then it declares none of them, and the receiving side offers only the rooms it lists:

| Platform | What it needs |
|---|---|
| KakaoTalk | its friends list, read through the desktop automation that reads the owner's account; a start opens the friend's chat on the desktop. Its account-safety rules forbid adding a friend automatically, so a contact is only a friend the owner already has, and a group start is declared only where the desktop can make one without adding anyone |
| Discord | a contact is a user who shares a server with the bot; a one-to-one is the bot's direct-message channel with that user, which the bot account opens itself. The bot cannot start a group of several users, so `startGroup` stays undeclared |
| Microsoft Teams | contacts from Microsoft Graph's people or users of the organization, and a start through Graph's chat creation, a one-to-one or a group chat with a topic, which needs the permission to create chats on the account it reads |

### Fetching files again

A provider that declares `fetchMedia` fetches the pictures and files a message carried from the platform
again, given the message's room and number and, for a platform that finds an old message by its time,
`sentAt` in epoch milliseconds. The fetch comes to the message files (see Files), to `unavailable` with the
platform's reason in English, or to a refusal. A provider that does not declare `fetchMedia` refuses with
`unsupported-capability`, which is also what the base answers for a provider that supplies no fetch of its
own. Only the files cross as an answer; `unavailable` and a refusal cross as errors (see Refusals).

## Messages

`HandedMessage` is a message as it crosses from a provider to the receiving side, under its room and its
number. Nothing platform-shaped crosses:

| Field | Type | Meaning |
|---|---|---|
| `origin` | `MessageOrigin` | `live` for a message a room said, `edit` for one already handed over and changed, `history` for one read back from the room's past |
| `senderId` | number or null | who said it |
| `senderName` | string | the name the platform shows for who said it |
| `sentAt` | number | when, in epoch milliseconds |
| `text` | string | its words, or a placeholder the provider wrote for what was sent |
| `replyToMessageId` | number or null | the message it answers |
| `via` | string | the key of the provider that took it in |
| `album` | number or null | the number of the first message of the send of several this one came in |
| `mode` | `ChatVoice` | the mode that delivered it |
| `context` | `InboundChatContext` | what the platform said about it beyond the message itself |
| `room` | `RoomFacts` | what the provider already knows of the room |
| `files` | list of message files | the files it carries, where they are already gathered |
| `gathering` | boolean | the provider is still gathering its files and sends them later |
| `gatherFailure` | string or null, optional | why the files gathering promised could not be kept |

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
| `tag` | the opaque tag of the send this message is the platform's echo of, where the send gave one |

### Room facts

A hand-over carries what the provider already knows of the room as `RoomFacts`, so the receiving side need not
ask for it: `kind` (`RoomKind`), `identity` and `privateTo`, meaning the same as in a `ProviderRoom`. Each is
optional, and the receiving side can always ask the room route instead.

### Files

A file crosses as a file item: `name`, `mime` and `bytes`. A file a message carried crosses as a message file:
a file item, `id`, the provider's identifier of that file among its message's files, and optionally `kind`.
The id is an opaque string. The same file carries the same id each time it crosses, whichever hand-over or
fetch carries it, and no two files of one message share an id, so the receiving side tells a file it already
keeps from another of the same name by the message and the id alone. How a provider makes the id is its own:
the platform's own name for the file where it has one, or the file's place among its message's files where it
does not.

`kind` is the `ChatFileKind`, what the file is as the platform showed it, so a voice note is told from a sound
file and a round video from a plain one:

| Value | Meaning |
|---|---|
| `photo` | a picture |
| `video` | a video |
| `round-video` | a short video the platform shows in a circle |
| `animation` | a short looping video without sound, or an animated image |
| `audio` | a sound file |
| `voice-note` | a recording made in the room to be heard as speech |
| `sticker` | a sticker |
| `document` | any other file |

A provider fills `kind` as far as its platform tells it and leaves it out where it cannot tell. An inline file
item of a send carries no `kind`.

A provider gathers a file from its platform. Where it is kept is the receiving side's concern, so the provider
hands the bytes over:

- when the files are already gathered, as the message's `files`;
- when they are still on their way, the message says `gathering` and a `GatheredPatch` follows: its `files`,
  or, where they could not be kept, no files and the reason as `gatherFailure`.

## The receiving side, as a provider sees it

`ChatReceiver` is the receiving side as a provider calls it, and a provider asks nothing else of it:

| Call | What it hands over or asks |
|---|---|
| hand over | a message under its room and number, whatever its origin; the answer is the message as held, and whether the receiving side held it before |
| gathered | the files a message's `gathering` promised, or why they were not kept |
| avatars changed | the rooms and people whose kept profile picture changed, as an `AvatarChange` |
| reactions changed | the reactions on one message as the platform shows them now, as a `ReactionChange` |
| deleted | messages of one room deleted on the platform, as a `Deletion` |
| room | one room as the receiving side holds it, a `ReceiverRoom`: whether it listens to the room, whether it asked for the room to be read, and the oldest message it holds of it; none where it holds nothing of the room |
| rooms | a page of the provider's rooms whose past it may read back, each a `ReceiverRoom` |

The receiving side keeps what it is handed whatever its origin. A provider never hands a message over as
already kept.

An edited message is held as the hand-over carries it: the receiving side takes its words and its sender from
the hand-over, so a provider that learns a message's sender was another (an account the owner took over, for
example) hands the message over again as an `edit`, under the right sender.

A message's reactions are handed over as the platform shows them: for each person listed, the emoji that person
has on the message now, none where they took theirs off; a person not listed keeps what the receiving side holds,
unless the change says it is the message's `whole` set, when a person not listed has none. A person is named by
their number, and the account itself by none.

A message deleted on the platform is handed over as deleted: the receiving side marks it so and keeps what it
held of it, its words included, so whoever reads the room can still open them.

Where the platform numbers a room's messages apart for each account, so the account that learns of a reaction
or a deletion knows the message by a number the receiving side never held, the message is named by its
`stamp` instead: who sent it and the moment it was sent, as the hand-over carried them. The receiving side
takes the message of that room it holds under that sender and that moment. A provider hands over only a
deletion it learned from the platform, never one it made itself on the receiving side's word.

A provider asks whether a room is listened to where it must decide by itself what to read or gather there; it
keeps no switch of its own for it. Being listened to is no request to read a room: the receiving side may
listen to every room it can hold, added or not.

`read`, false unless the receiving side says otherwise, is whether the receiving side asked for the people's
words of the room to be read through the account of the provider asking; which rooms it asks for is its own. It
is the one opt-in a room needs for a provider to read the people's words of a room it sees only through a
person's account (a read-only room, its `modes` holding `human-reader` alone): a provider reads
them only where `read` is true, and hands them over as `live` in the `human-reader` voice. Where `read` is false
it reads in such a room only what it would read anyway, the person's own words. A receiving side that does not
say answers false, so no room is opened to reading by a side that never asked for it.

A provider that reads rooms' past back, one that declares `history`, asks which rooms it may read back and
from where, and hands each past message over as `history`, never as `live`, so nothing is woken for words said
long ago. Its provider key, `via`, names the provider in every call that is not about a message it carries.

## Sends

A send is one kind at a time. Every send carries `roomId`, its `kind` and the kind's own `fields`, and may
carry:

- `mode`: the voice to speak with, for a provider that reads by more than one (a send in the owner's own words
  names `human-reader`). Unnamed, the provider speaks with the voice it answers the room by. A voice that is
  not active is refused, with the reason.
- `speaker`: who speaks, where it is not the one the room is answered by, as the caller's opaque string. Only
  a provider that declares `speakers` takes it.
- `tag`: an opaque string the provider returns on the inbound message that is the platform's echo of this
  send. It is meant for a provider whose platform gives no message number back for a send.
- `prefix`: the words the answers carry before them in the `human-avatar` voice, which the caller supplies. A
  provider puts them before the words of a send that goes out in that voice, and ignores them in any other
  voice.
- `replyToMessageId`: the message the send answers. A provider that does not declare `replyTo` ignores it, and
  so does every kind that answers no message: `reaction`, `pin`, `typing`, `edit` and `delete`.

| Kind | Its `fields` |
|---|---|
| `text` | `text`; optional `silent` (a notice that must not buzz a phone), `fold` (fold where the dialect can) |
| `photo` | `imageUrl`; optional `caption` |
| `album` | `items`, each either `{ address }` (a web address or a file path the provider can read) or an inline file item `{ name, mime, bytes }`, which carries no id; optional `caption` |
| `document` | `bytes`, `fileName` |
| `audio` | `audio`, `fileName` |
| `voice` | `audio`, `said` (what the line said); optional `caption`, `transcript` (what was actually read out) |
| `sticker` | `emoji` |
| `reaction` | `messageId`, `emoji`; an empty emoji takes the account's own reaction off |
| `pin` | `messageId`, `on`; optional `silent` |
| `typing` | `on` |
| `edit` | `messageId`, `text` |
| `delete` | `messageId` |

Sanitising the words and deciding whether to fold them happen before the send reaches the provider. A provider
takes the words as they come and applies only its dialect. Keeping a copy of what went out is the sender's own
concern: the sender holds the bytes it sent, and the send's result gives it the message number to keep them
under.

### Results

A send comes to one of:

- **sent**: a `ChatSent`, which is `messageId` (null where the platform hands none back) and `missed`, the
  pictures of an album that did not go, each a `MissedPicture`, empty where everything went. This is the
  send's answer.
- **refused**: a `ChatRefusal` (see below), which crosses as an error.

A send that the provider does not perform is refused and never dropped. A provider that fails while
performing a send answers with the error shape in `protocol.md`.

`MissedPicture` is a picture by the `address` it was given as, with a `reason`.

## Refusals

Every refusal is a fixed English sentence and a code a caller can match on. Translation into a person's
language happens wherever a person reads it, never here. The sentence takes `label`, the provider's label, and
the variant's own fields.

A refusal crosses the wire in the error shape of `protocol.md`: its code as `code`, its sentence as `error`,
with the status below. The fields stay with the provider, which renders them into the sentence; a caller
matches on the code and shows the sentence.

`ChatRefusal`:

| Code | Status | Fields | Sentence |
|---|---|---|---|
| `unsupported-capability` | 422 | `label`, `capability` | "{label} cannot {what}." where {what} is the capability's words below |
| `no-picture-sent` | 422 | `label`, `missed` | "{label}: no picture was sent." followed by one line per missed picture, "{address}: {reason}" |
| `speaker-not-allowed` | 403 | `label`, `speaker` | "{label} cannot send in the name of {speaker}." |
| `speaker-unnumbered` | 422 | `label`, `speaker` | "{label} has no number for {speaker}, so it cannot send in that name." |
| `not-ready` | 503 | `label` | "{label} is not ready yet." |
| `platform` | 422 | `label`, `detail` | "{label}: {detail}" where {detail} is the provider's own English reason |
| `gone` | 410 | `label`, `detail` | "{label}: {detail}", for a message the platform no longer has or will never let change again, so the caller lets go of it instead of asking again |

A fetch the platform would not give is no refusal: it crosses as 404 `unavailable`, with the platform's reason
as `error`.

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
| `roomList` | list the rooms of its account |
| `contactList` | list the contacts of its account |
| `startChat` | start a conversation with a contact |
| `startGroup` | start a group conversation |

`MissedPictureReason`, the `reason` of a missed picture:

| Code | Sentence |
|---|---|
| `not-an-address` | "It is neither a web address nor a file path on this machine." |
| `outside-data-root` | "The file is outside the data folder, so it was not sent." |
| `missing-file` | "There is no such file on this machine." |
| `unfetched` | "The picture could not be fetched." |
| `platform` | the provider's own English reason, carried as `detail` |

A file a provider could not keep crosses as the `gatherFailure` of a `GatheredPatch`, with the reason in
English.

## What is deliberately not here

| What | Where it belongs |
|---|---|
| the announcement a room hears when it is shut | the receiving side |
| refusal and placeholder text that belongs to one platform | that platform's provider repository, in English |
| compositing pictures (laying a transparent image on a plain ground, for example) | whichever provider repository needs it |
| numbering algorithms and the tables that hold assigned numbers | the provider that assigns them |
| keeping, routing and waking on what rooms say, room rules, permissions and windows | the receiving side |
| checks on outgoing words, sanitising, fold decisions and composing the words | whoever writes the sends |
| the names behind a speaker | whoever names the speaker |
| translating sentences for a person | wherever a person reads them |
