---
name: bmo-chat
description: Drive a chat provider through the chat protocol with the bmo-chat MCP server. Use when building, testing or debugging a chat service that serves the chat protocol's provider routes, when reading what it knows of its rooms, or when checking what its dialect makes of markdown, with no receiving side running.
---

# bmo-chat

A chat provider holds one platform's connection and serves the chat protocol's provider routes. The
`bmo-chat` MCP server drives any provider at a given address through those routes, one tool for each, so a
provider can be read and checked with nothing else running.

No tool sends. The server offers nothing for the provider's `POST /sends`: words reach a room only through the
receiving side, never from a harness past it.

## Starting the server

Register the server with the harness as a stdio MCP server:

```
bmo-chat mcp --provider http://127.0.0.1:8080
```

The address is the provider's base address, where its `/declaration` route stands. The provider's bearer
token goes in the `BMO_CHAT_TOKEN` environment variable of the server, or `--token`; every route needs it,
and a wrong one answers every tool with a 401 `unauthorized` error.

## What to do first

1. `chat_declaration`: read what the provider says of itself. `configured` is null when it can run, else why
   not. `can` lists the kinds of send it takes and the answers it gives (`replyTo`, `fetchMedia`, `avatars`,
   `history`, `roomList`, `contactList`, `memberList`, `startChat`, `startGroup`). `dialect` says what markdown becomes in its rooms
   and how long a message may be. `modes` says which accounts it reads by and whether each is running.
2. `chat_health`: whether it is ready now. While it is not, the rooms, start, contacts, room, members, owner and file tools
   answer `not-ready`.
3. `chat_owner`, then `chat_room` on the room it names, to see what the provider knows of a room: whether it
   owns it, the account's own sender number there, whether it is the owner's private chat, and its title.
   Where it declares `roomList`, `chat_rooms` pages the rooms its account is in, each answered the same way;
   pass the last one's `id` as `after` for the next page while `more` is true. Where it declares
   `contactList`, `chat_contacts` pages the contacts of its account the same way, each with the one-to-one the
   account already has with it, or null; `accountId` names another account of the provider's to list instead.
   Where it declares `memberList`, `chat_members` lists the people in a room whole, each with the sender number
   the provider gave them or null, their name and status line; `not-found` carries the provider's reason where it
   holds none of the room's members.

`chat_start` starts a conversation on the platform, `{ "body": { "contactId": 5 } }` for a one-to-one or
`{ "body": { "title": "…", "contactIds": [5, 6] } }` for a group, either with `accountId` beside it to start it in
another account of the provider's. It is real: a group is made on the platform and its people see it, so
start one only with contacts set aside for testing.

## Checking the dialect

To see what formatted words become, `chat_dialect_reading` with `{ "body": { "text": "<markdown>" } }` shows the
markup that would go out, the plain words, the parts each fitting one message, and the fold.
`chat_dialect_check` with `{ "body": { "markup": "<markup>" } }` says whether markup the dialect built is
sound and what a reader sees of it. Neither sends anything.

## Files and pictures

`chat_message_files` fetches again the files a message carried, by `roomId` and `messageId`, with `sentAt` for
a platform that finds an old message by its time; the provider must declare `fetchMedia`. `chat_avatars` pages
the profile pictures the provider keeps, oldest taken first; pass the last one's `takenAt` as `after` for the
next page while `more` is true.

## When a tool answers an error

A tool error's text is JSON: `status`, `code` and `error`, the provider's English sentence. Match on `code`:

- `unsupported-capability`: the provider does not declare what the call needs; read `can` again.
- `not-ready` (503): the provider is up but not ready; ask `chat_health` why, and try again later.
- `platform`, `gone`, `unavailable`: the platform refused, or no longer has the message or
  file; the sentence says why. Let go of a `gone` message rather than asking again.
- `not-found`: nothing stands at the room or picture asked for.
- `bad-request`: the arguments or the body are not the shape the tool takes; the sentence names what is wrong.

An error with no `code` means the provider could not be reached at all; check the address and that the
provider is running.

The routes and pieces behind each tool are stated in the chat protocol's `docs/protocol.md` and
`docs/contract.md`, and in `docs/openapi/provider.json`, whose `x-tools` lists every tool's input and output.
