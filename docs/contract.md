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

`ChatCapability` is the twelve kinds plus eleven answers that are not sends:

| Capability | Meaning |
|---|---|
| `replyTo` | an answer can be aimed at a particular message |
| `fetchMedia` | a picture or a file can still be fetched after its message arrived |
| `avatars` | the provider supplies profile pictures |
| `history` | the provider can read a room's past messages back |
| `roomList` | the provider can list the rooms its account is in |
| `contactList` | the provider can list the contacts of its account |
| `memberList` | the provider can list the members of a room |
| `startChat` | the provider can start a one-to-one with a contact of its account |
| `startGroup` | the provider can start a room of several contacts of its account, under a title |
| `peopleTyping` | the provider tells when a person is writing in a room (see "Typing and presence") |
| `presence` | the provider tells whether a person is reachable (see "Typing and presence") |

The full capability set is all twenty-three, taken from the type itself, so a reader handed a capability's name
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
| `registration` | the app registration a person's own account needs before it reads on the provider, where it needs one: `url`, where it is made, and `fields`, the values it gives, which the person keeps among their secrets; left out where it needs none |
| `nodeProbe` | how a person's node is asked whether it holds what the provider reads a person's account through there, where it reads one through a node: `powershell` and `linux`, a command for each shell a node takes one in, each exiting 0 where the node holds it and 1 where it does not, any other end telling nothing, and `missing`, what the person installs or signs into where it does not; left out where the provider reads no account through a node |

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

### The account a call is for

A provider may speak through more than one account on its platform, one for each person it answers for. A
send, a start and a listing of contacts may therefore name `accountId`: the account the call is for, as the
number the caller knows that account by. The base never reads it and keeps no list of accounts; which of its
platform's accounts a number stands for, and how it learns that, is the provider's own.

- Left out or null, the call is for the provider's own account, the one it holds where it holds no other.
- A provider that holds its own account alone takes every call as for that account, whichever it names.
- A provider that holds an account for each person acts in the account named and never in another. Where it
  cannot act for that account, a send or a start is refused `platform` with its own reason, and a listing of
  contacts fails rather than answer another account's.

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
conversation with, so a person can choose among them. A listing may name the account it is for (see "The account
a call is for"); what its platform counts as a contact is the provider's own. Each is a `ChatContact`:

| Member | What it is |
|---|---|
| `id` | the contact, as a number of the provider's `people` space |
| `name` | what the account calls the contact |
| `kind` | `ChatContactKind`: `person` for a person's account, `bot` for a platform's bot account |
| `roomId` | the one-to-one the account already has with the contact, as a number of the provider's `rooms` space, or null where it has none |

The base pages the list in ascending order of contact number and asks only a provider that declares
`contactList`; one that does not is refused `unsupported-capability` without being asked. As with a listed room,
whether the receiving side holds a contact's `roomId` is the receiving side's to mark, never the provider's.

### The members of a room

A provider that declares `memberList` lists the people in a room as its platform last told it, so a person can
see who is there. Each is a `ChatMember`:

| Member | What it is |
|---|---|
| `person` | the person, as a number of the provider's `people` space, the number the room's messages carry as their sender; null where the provider has given the person none |
| `name` | what the room calls the person |
| `statusMessage` | the person's status line, or null where the platform gave none |

The list is answered whole, as `members`, in the order the provider keeps it. A provider gives no number to a
person only to list them, so a person who has not yet spoken in any of its rooms may stand with none, and no
platform's own identifier ever stands in for one. The base asks only a provider that declares `memberList`; one
that does not is refused `unsupported-capability` without being asked. A room whose members the provider does not
hold, because it knows nothing of the room or has not yet read its members, is answered `not-found` with the
provider's own reason.

### Starting a conversation

A provider that declares `startChat` or `startGroup` starts a conversation on its platform when asked. A
`ChatStartRequest` is a `ChatStart` and, optionally, the `accountId` it is for (see "The account a call is
for"). A `ChatStart` is one of two shapes:

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
| KakaoTalk | its friends list, read headless from the platform's `talk/friends/sync` over the owner's account's login; a one-to-one start is the chat protocol's `CREATE` naming the one friend's id, and a friend who already has a one-to-one is answered the room the account's room list names. Its account-safety rules forbid adding a friend automatically, so a contact is only a friend the owner already has; they also forbid every start that invites people, a `CREATE` naming several ids, `ADDMEM` into a room that stands and the open-chat commands, so `startGroup` is never declared |
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
| `text` | string | its words, or a placeholder the provider wrote for what was sent; on a line, the sentence that says it to a reader that does not render the line |
| `line` | `MessageLine`, optional | the line the platform said, where the message is one rather than anyone's words (see Lines) |
| `replyToMessageId` | number or null | the message it answers |
| `via` | string | the key of the provider that took it in |
| `album` | number or null | the number of the first message of the send of several this one came in |
| `mode` | `ChatVoice` | the mode that delivered it |
| `context` | `InboundChatContext` | what the platform said about it beyond the message itself |
| `room` | `RoomFacts` | what the provider already knows of the room |
| `files` | list of message files | the files it carries, where they are already gathered |
| `gathering` | boolean | the provider is still gathering its files and sends them later |
| `gatherFailure` | string or null, optional | why the files gathering promised could not be kept |

A message carries no file path. The files a message carries cross the wire as their bytes, inline or as parts
(see Files).

### Lines

Some messages are no one's words but a line the platform says of the room, such as people coming in or going
out. A provider hands such a message over with `line`, a `MessageLine`: `code`, a kebab-case string naming the
line, and `fields`, an object of what it carries. A reader renders the line in its own words from the code and
the fields; `text` still carries a plain sentence saying the same, which a reader that does not render the line,
or does not know its code, shows instead. A message with no `line` is someone's words, and a reader keeps a code
it does not know as it came, with its fields, rather than refusing the message.

A person a line names crosses as `senderId`, the person's number in the room's sender numbering as `senderId`
on a message, and `name`, the name the platform shows for them. The protocol states these codes, and a provider
whose platform tells of such an event hands it over under them:

| Code | Fields | When |
|---|---|---|
| `people-joined` | `people`: a list of persons, at least one; `by`: a person, optional | people came into the room; `by` is who brought them in, given only where that is someone other than the people who came |
| `person-left` | `person`: a person; `by`: a person, optional | a person went out of the room; `by` is who took them out, given only where that is someone other than the person who went |

The provider library builds each of these with its own constructor, so a provider writes no field names of its
own.

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

### A file's bytes

A file's bytes are a `FileBody`, held one of three ways:

| Held | What it is | How it crosses |
|---|---|---|
| inline | the bytes in memory | inline, as base64 |
| stored | a file on the disk of the side that holds it, read only as it is sent | as a part of a multipart body (`protocol.md`, "Files carried as parts") |
| withheld | no bytes: a file named by an answer whose asker already holds them | with its `bytes` left out |

- **A sender picks the way.** A file already on its disk is sent stored, so it is never read whole into
  memory; a small one may be sent inline. A stored file the sender names is read where it lies and left there.
- **A part arrives stored.** The side a part arrives at writes it, as it arrives, into a directory of its own
  choosing on its own disk (its spool), the system's temporary directory where it names none, and hands the
  file on stored there. The file is removed once the last holder of it lets go, unless it was moved away
  first, so a side that keeps it moves it where it keeps files.
- **A side reads a file's bytes as it needs them:** whole, or as a stream read from the disk. Reading a
  withheld file is an error.
- **Every body that carries files takes them both ways:** a send's files, a hand-over's, a gathering's patch's
  and the answer of a fetch of a message's files, which the provider answers as it holds them and the caller
  receives as they cross: a part stored in its own spool, an inline file in memory. The answers of a
  hand-over and of a patch name their files withheld.

A provider gathers a file from its platform. Where it is kept is the receiving side's concern, so the provider
hands the bytes over:

- when the files are already gathered, as the message's `files`;
- when they are still on their way, the message says `gathering` and a `GatheredPatch` follows: its `files`,
  or, where they could not be kept, no files and the reason as `gatherFailure`.

## The receiving side, as a provider sees it

`ChatReceiver` is the receiving side as a provider calls it, and a provider asks nothing else of it:

| Call | What it hands over or asks |
|---|---|
| hand over | a message under its room and number, whatever its origin; the answer is the message as held, its files withheld, and whether the receiving side held it before |
| gathered | the files a message's `gathering` promised, or why they were not kept |
| avatars changed | the rooms and people whose kept profile picture changed, as an `AvatarChange` |
| reactions changed | the reactions on one message as the platform shows them now, as a `ReactionChange` |
| deleted | messages of one room deleted on the platform, as a `Deletion` |
| typing marked | a person writing in a room now, or no longer, as a `TypingMark` (see "Typing and presence") |
| presence changed | whether people are reachable now, as a `PresenceChange` (see "Typing and presence") |
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

## Typing and presence

Who is writing in a room now, and who is reachable now. Neither is a message: a provider hands neither over as
one, and the receiving side keeps neither in a room's record. `presence.md` states the design across every side;
this section states what a provider tells.

### A typing mark

A `TypingMark` is a person writing in a room now: `via`, `roomId`, `personId` (a number of the provider's
`people` space), `on`, and `lastsMs`, how long the mark stands unless it is told again.

- **A mark ends by itself.** A mark told `on` and not told again ends when its life runs out, so a lost off
  never leaves anybody shown typing. Told `on` again, it stands for a new life from then, so a platform that
  tells a mark again while a person keeps writing has each one told as it comes.
- **It ends at once** when it is told off, when the same person's message reaches the same room, and when the
  provider that told it stops being ready. A platform that tells no off leaves the mark to end by its life.
- **Its life is the platform's own.** A provider names the life its platform gives a mark as `lastsMs`; one that
  names none (`lastsMs` null) gets the base's default of six seconds, which both libraries carry as
  `TYPING_MARK_LASTS_MS`.
- **The life ends a mark's showing, not any work.** Nothing waits on a mark and nothing is stopped by one.

### Presence

`ChatPresence` is whether a person is reachable now: `state`, a `ChatPresenceState`, and `lastSeenAt`.

| `ChatPresenceState` | Meaning |
|---|---|
| `online` | the person is at the platform now, busy or not |
| `away` | the person is signed in but idle, or stepped away |
| `offline` | the person is not at the platform, or hides being there |

- **A platform's own states are mapped by its provider:** a busy or do-not-disturb state is `online`, an idle or
  be-right-back state is `away`, an invisible one is `offline`. Nothing outside the provider reads the platform's
  own words.
- **`lastSeenAt`** is the moment the platform says the person was last at it, or null where it does not say. A
  platform's coarse word in place of a moment (recently, within a week) is no moment, and crosses as null.
- **Presence stands until it is told again.** It is never worn out by time. A provider that stops being ready
  leaves the presence it told unknown, not `offline`, until it tells again.

A `PresenceChange` tells it: `via`; `people`, each a `PersonPresence` of `personId` beside the person's
`ChatPresence`; and `whole`, true where the list is every person the provider tells presence of, so a person not
listed is then unknown. A provider tells a whole list each time it becomes ready, so a receiving side that
restarted, or a provider back from an outage, stands on the present at once.

### What a provider tells

- **A provider declares `peopleTyping` and `presence` only while the road that hears each runs,** as it declares
  any capability that stands on one road. One that hears neither declares neither and tells nothing.
- **The link drops a mark or a change whose capability the provider does not declare,** without sending it, so
  a provider never has to check its own declaration before telling. The link reads the declaration of the
  provider it is told it serves, as that provider declares now; a link told of no provider, or of one no longer
  running, takes it as declaring neither.
- **A provider tells only of rooms the receiving side listens to.** In a read-only room it tells only where the
  receiving side asked for the room to be read (`read`), since a person's typing there is as much theirs as their
  words.
- **It tells the presence only of people who stand in such a room,** as a sender of its messages or one of its
  members. A person the account knows only from elsewhere (its contacts, a room nobody listens to) is never told
  of.
- **The account a provider reads by is never told of.** Its typing and its presence are known to the side that
  drives it already.
- **A mark or a change about a room the receiving side holds nothing of is passed over,** and still answered as
  taken: neither is anything a provider can repair.
- **The account's own typing goes out as the `typing` send** (see "Sends"); no presence goes out, since a
  person's presence on a platform is set by that person's own use of it.

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
- `accountId`: the account the send goes out in (see "The account a call is for").

| Kind | Its `fields` |
|---|---|
| `text` | `text`; optional `silent` (a notice that must not buzz a phone), `fold` (fold where the dialect can) |
| `photo` | `imageUrl`; optional `caption` |
| `album` | `items`, each either `{ address }` (a web address or a file path the provider can read) or an inline file item `{ name, mime, bytes }`, which carries no id; optional `caption` |
| `document` | `bytes`, `fileName` |
| `audio` | `audio`, `fileName` |
| `voice` | `audio`, `said` (what the line said); optional `caption`, `transcript` (what was actually read out) |
| `sticker` | either `emoji` or `item` (see below) |
| `reaction` | `messageId`, `emoji`; an empty emoji takes the account's own reaction off |
| `pin` | `messageId`, `on`; optional `silent` |
| `typing` | `on` |
| `edit` | `messageId`, `text` |
| `delete` | `messageId` |

Every field that carries a file's bytes (`bytes` of a document and of an inline album item, `audio` of an audio
and of a voice) is a file body (see "A file's bytes"), so a send of a large file carries it stored.

A sticker is named in one of two ways. `{ emoji }` names an emoji, for which the provider picks a sticker of
its own. `{ item }` names one sticker item of the platform's own, which the provider maps onto its platform's
wire:

| Field of `item` | Meaning |
|---|---|
| `id` | the item's own key on the platform |
| `pack` | optional: the key of the pack, set or product the item belongs to |
| `title` | optional: the item's title, or the title of its pack |
| `imageUrl` | optional: the address the item's picture stands at |
| `thumbnailUrl` | optional: the address of the item's small picture |

A provider that declares `sticker` but can send only one of the two forms refuses the other `platform`, with
its own reason; so does one whose platform needs a field of `item` the send leaves out.

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
| `memberList` | list the members of a room |
| `startChat` | start a conversation with a contact |
| `startGroup` | start a group conversation |
| `peopleTyping` | tell who is typing in a room |
| `presence` | tell who is online |

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
