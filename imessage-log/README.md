# imessage-log

Print the Messages app's history from the terminal: every conversation in a
span of days, or only the ones with one person.

```
$ imessage-log --from 2026-04-09 --to 2026-04-10 --contact ada
── 2026-04-10, Friday
19:00  Ada Lovelace: Where shall we eat?
19:05  me → Ada Lovelace: The usual place
20:00  [Book club] Ada Lovelace: [attachment]
```

| Option | |
|---|---|
| `--from`, `--to` | The days, inclusive. Default: the last week |
| `-c`, `--contact` | Conversations with someone whose name, number or address contains this. Numbers match however they are written — `070-000 00 01` finds `+46700000001`. Group chats they are in count too |
| `--direct` | With `--contact`, leave out group chats |
| `-t`, `--text` | Messages containing this |

Tapbacks and other reactions are left out. An attachment shows as
`[attachment]`; what it was is not read.

## Full Disk Access

`~/Library/Messages/chat.db` and the Contacts databases are protected by
Full Disk Access, and a program gets that from the terminal it runs in.
Rather than granting it to the terminal you use for everything, keep one
terminal app just for this. Its shell may not have `~/.cargo/bin` on its
`PATH`, in which case call the tool by its full path.

Both databases are opened read-only. Nothing is written, sent or cached.

## How it reads

The Messages database is Apple's own SQLite file, with an undocumented
schema that has been stable for years: `message`, `handle`, `chat`, and the
joins between them. Dates are nanoseconds since 2001-01-01. Since macOS
Ventura, many messages keep their text only in `attributedBody`, an
`NSAttributedString` in Apple's old typedstream format; the text is the
first `NSString` in it, and that is what is decoded.

Names come from `~/Library/Application Support/AddressBook/`, one database
per account, all read. A number without a country code is taken as Swedish.
