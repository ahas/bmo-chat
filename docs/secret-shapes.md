# The secret shapes

`bmo-secret-shapes`, in `secret-shapes/`, is the one list of the shapes a secret's run is written in and the
one reading that takes such runs out of words. A unit that keeps or forwards words a person or an agent wrote
reads them through it, so every unit takes out the same runs, and none keeps a list or a reading of its own.
The crate knows no unit and depends on nothing `bmo-*`.

## The list

- **The shapes are data,** the crate's `shapes.toml`, compiled into the crate. Each shape has a name and a
  regular expression as the `regex` crate reads one, with no look-around, and may list the runs it spares.
- **They are read in the order the list holds them,** each over the words the shapes before it left, so a run
  a narrower shape took is a marker by the time a wider shape reads it.
- **Where a shape names a part of its run `secret`, that part alone is replaced,** so a `name=value` keeps its
  name and a link keeps its scheme, user and host.
- **A shape may name the runs it spares,** each a regular expression of its own: a run of the shape that lies
  wholly inside a run of one it spares stands, and every other shape still reads it. A spared pattern reads
  the words with every marker in them read as a run of letters, one to each of the marker's bytes, so a run an
  earlier shape took out of a link leaves the link whole to its spare, whatever marker the reader names.
- **A shape is narrow by design:** a run an ordinary sentence could hold, in any script, is no secret's shape,
  and a commit's forty-character hash stands.
- **As the list stands it holds** a PEM private key block, to its end or to the end of the words where it has
  none; a JWT; the provider keys written `sk-` and a run of twenty or more after it; GitHub's personal, OAuth,
  user, server and refresh tokens and its fine-grained personal access token; Slack's tokens; an AWS access
  key id; a Google API key; a Hugging Face token; a Telegram bot token; the password in a link's user part,
  `scheme://user:password@host`; the credentials after `Authorization: Basic`; the token after `Bearer`,
  twenty or more characters; the session strings Telethon and Pyrogram write, a run of three hundred or more
  base64 letters, which spares an http(s) link since such a run inside a link is the link's own; a platform's
  sixty-four hexadecimal digits standing alone; and a name ending in `key`, `token`, `secret`, `password`,
  `passwd`, `sig` or `signature`, in any case, set with `=` or `:` to a value of twelve or more characters,
  which spares a console secret-request link: a link to the console's page of a room whose `secret` parameter
  is a secret's file name, with nothing beside it but the `kind`, `fields`, `for` and `user` parameters.

## The reading

- **`redact` takes the words and the marker,** and answers the words with each run replaced by the marker,
  borrowed as they were where no run was found, and how many runs each shape replaced, one entry to a shape
  that replaced any, in the order the shapes are read, by the shape's name. It says nothing of a value it took
  out.
- **The marker is the reader's.** What a reader sees in a run's place is its own word; `MARKER`,
  `<redacted-secret>`, is the one a reader with no word of its own takes. A marker is to be no shape's run, so
  words read twice lose nothing more; a run that is the marker itself is never replaced.
- **`shapes` lists the shapes compiled,** each by its name, in the order they are read, for a reader that
  reports by shape or checks it knows them all.
- **Every shape is compiled once, on the first reading.** The crate's own tests compile the whole list, so a
  shape that does not compile fails them and never reaches a reader.
- **Reading JSON is the reader's.** A reader that reads a payload reads each of its strings through
  `redact` and decides itself which keys and fields stand.

## Tests

The crate's tests hold every shape to a sample of its run, assembled at run time so no tracked file holds a
secret's value, and hold ordinary words, a commit's hash, a long sentence in another script and the spared
links to standing as they were written. The sentence in another script stands in a fixture file of its own.
