# Personal data

No personal data stands in this repository. It is not written into code, comments, documents, test
fixtures or commit messages, and no example in them is a real value.

## What it is

- A person's name, handle or account id.
- A mail address, host name, domain, home path, device or other network information.
- A room member's display name or id.
- A company's name.
- A line of a real conversation.

## What to write instead

- A value that differs by person, room, host or deployment is configuration its consumer passes in; it is
  never a constant here.
- An example is neutral: a role such as `tester`, a made-up line, never a real name, account, address or
  a line someone actually wrote.

## The values the owner allows

Two values stand in this repository because they name its owner and maintainer: `ahas`, and the mail
address `dev@ahas.io`. The check admits them and no other personal value.

## The check

The repository test reads every file of the checkout and fails where it finds a mail address, a home
path, a phone number, a token or a private address. It reports the file, the line and the kind, never the
value it found. Files git ignores, such as the build output, are not read.

The repository holds the forms, not a list of terms. A word that names a person or a company is never
written here for a scan to look for; a writer follows the rule itself.
