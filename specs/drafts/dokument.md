# dokument: a catalogue of receipts and letters, and receipts in kontoutdrag

Receipts and letters arrive from several places, each with a tool of its
own that downloads them into a folder: a mirror of a Kivra mailbox made
by [kivra-sync](https://github.com/felixandersen/kivra-sync), a folder of
Willys digital receipts, and documents filed by hand. Each of those
folders has a layout its tool owns, because the tool decides what to
download again by looking at what is already there. Nothing reads across
them.

This adds `dokument`, a tool that reads those folders through one
adapter per source and keeps a single catalogue: what each document is,
when, from whom, for how much, and where its PDF and its text are. It
extracts the text without a language model, so building and searching
the catalogue costs nothing. `kontoutdrag` then uses the catalogue to
link a card transaction to its receipt, in `list` and in the view.

Two tools, one spec, because the catalogue's format is the contract
between them and is only worth fixing once there is a reader.

## Functionality

### Sources

`dokument` reads `~/.config/skagedal-tools/dokument/settings.toml`
(`dokument edit-config` opens it, from a commented template like
kontoutdrag's):

    [[source]]
    kind = "kivra-sync"
    path = "~/Documents/kivra"

    [[source]]
    kind = "willys"
    path = "~/Documents/receipts/willys"

    [[source]]
    kind = "folder"
    path = "~/Documents/economy"

The paths above are examples; the real ones live only in settings.

- **`kivra-sync`** reads the tool's output directory: `Receipts/json/` and
  `Letters/json/`, one JSON file per item, beside the PDFs. A receipt's
  JSON carries the total, the purchase time to the minute, the store and,
  for card payments, the masked card number. A letter's carries the
  sender, the subject, the time received and its parts.
- **`willys`** reads the `purchases.json` the Willys download writes,
  which is the website's list of purchases as returned, and pairs each
  entry with its PDF. The entries whose `receiptSource` is `axcrm` have a
  `bookingDate` that runs early by the Stockholm offset from UTC (two
  hours in summer, one in winter); the adapter corrects it. That was
  checked against the time printed on the receipts.
- **`folder`** takes any directory of PDFs, recursively, and reads the
  date from an ISO date in the file name when there is one. It is for
  documents filed by hand, which have a README but no structured data;
  they are in the catalogue for search and are never matched.

A source that is configured but missing is a warning, not an error, so a
machine without the iCloud folder synced still builds.

### The catalogue

One record per document, in
`~/.local/share/skagedal-tools/dokument/catalogue.json`:

    {
      "id": "kivra:2601a1b2c3d4…",
      "source": "kivra-sync",
      "kind": "receipt",               // receipt | letter | document
      "time": "2026-01-15T17:42:08+01:00",
      "party": "Exempelbutiken",       // store or sender, as the source names it
      "subject": null,                  // letters only
      "amount": "-129.90",             // receipts only; signed as a bank would sign it
      "card": "1234",                   // last four digits, when the receipt shows them
      "items": [{"text": "…", "amount": "-129.90"}],
      "pdf": "/abs/path/….pdf",
      "data": "/abs/path/….json",      // the source's own record, when it has one
      "text": "/abs/path/….txt"        // in the cache, see below
    }

`id` is `<source>:<the source's own key>`: the Kivra key, the Willys
receipt reference, or the path relative to a `folder` source. It stays
the same across rebuilds, so marks and comments can refer to it.

`amount` is signed the way the bank signs it, negative for a purchase
and positive for a return. That way the matcher compares like with like,
and a return matches its refund.

`items` is filled for Kivra receipts, whose JSON has line items. It is
empty for Willys, whose PDFs would need parsing, and that is left out of
this spec.

### Text

`dokument build` writes a plain-text copy of every PDF to
`~/.cache/skagedal-tools/dokument/text/<id>.txt`. It is a cache: it can
be deleted and is rebuilt on the next `build`.

- A PDF with a text layer goes through `pdftotext -layout` (Poppler).
  That is nearly all of them.
- A PDF without one, detected as fewer than 50 non-space characters, goes
  through `dokument-ocr`. This is a Swift command-line helper in this
  repository that renders each page and runs Apple's Vision text
  recognition with Swedish and English. It runs on-device, costs nothing
  and needs no install beyond the tool itself. On Linux, and wherever the
  helper is missing, the record gets no text and `build` says how many.

Extraction runs only for documents whose PDF is newer than their text
file, so a rebuild after a sync touches only what is new.

### Commands

    dokument build                  read every source, update the catalogue and the text cache
    dokument list [--kind K] [--from D] [--to D] [--party P]
    dokument search <words>         documents whose text contains every word, newest first
    dokument show <id>              the record, and its text
    dokument open <id>              the PDF in the default viewer

`list` and `search` use the output conventions kontoutdrag has, with
`-o table|tsv|json`.

### Matching in kontoutdrag

kontoutdrag gains an optional setting:

    [documents]
    # The catalogue dokument builds. Receipts in it are linked to card
    # transactions in `list` and in the view.
    catalogue = "~/.local/share/skagedal-tools/dokument/catalogue.json"

It reads the file directly and does not depend on the `dokument` crate.
The shared record type lives in a small crate, `dokument-catalogue`,
which both use.

A receipt and a transaction match when:

1. **The amount is equal**, to the öre, as signed.
2. **The day is the same.** The receipt's local date equals the
   transaction's purchase date: `Descriptor::Card { purchased }` when the
   statement gives it, as Enable Banking's `transaction_date` does.
   Without a purchase date, the booking date may be up to three days
   after the receipt.
3. **The match is unique both ways.** The receipt matches no other
   transaction, and the transaction no other receipt.

If the third condition cannot be met, the pair is left unlinked rather
than guessed. When several candidates tie, the card digits break the
tie first, then how well the store name agrees with the merchant table's
name for the descriptor. A pair is linked only if exactly one survives.

Tried against three years of real statements, amount and day alone
linked about nine receipts in ten. Almost all of the rest had no
transaction to find: they were paid by another card, in cash, or from
an account whose statements were not loaded.

A mark can pin or refuse a link, for the cases the rule gets wrong:

    - date: 2026-01-15
      amount: -129.90
      document: kivra:2601a1b2c3d4…    # or `document: none`

The field joins `category`, `merchant` and `tags` as an effect a mark
can have. A pinned document is taken out of automatic matching, so it
cannot be linked twice.

### Where the link shows

- **`list`** gets a `document` column, blank when there is none. It is
  off by default and on with `--full`. `--explain` says whether a link
  came from the rule or from a mark. `-o json` carries the id.
- **`view`** carries the id and the party, time and items in each
  transaction's JSON. The expanded row shows the store, the time and the
  line items, with a button that opens the PDF. Two routes serve it:
  - `GET /api/document/<id>` serves the PDF, for `--serve` in a browser.
  - `POST /api/document/<id>/open` opens it in the default viewer, which
    is what the button does inside the webview, since WKWebView has no
    good answer for a PDF opened in a new window.

  Both routes look the id up in the catalogue and serve only a path found
  there, never a path taken from the request. `webview-shell`'s
  `send_body` becomes public, or gains a `send_file`, for this.

## Decisions

- **The fetchers stay where they are.** `kivra-sync` is someone else's
  tool, installed from a tap, and the Willys download works. `dokument`
  reads what they write and never writes into their folders, so a
  resync cannot be confused by anything it adds.
- **A catalogue file, not a database.** Documents number in the low
  thousands, the file is rebuilt in seconds, and JSON is readable by a
  person, by `jq` and by an agent.
- **The text lives in a cache, not beside the PDFs.** It can always be
  made again, and the source folders are often synced to a cloud drive,
  where a copy of every document as text would be a second thing to
  keep private.
- **Vision rather than Tesseract** for the few PDFs without text: no
  dependency, better Swedish, and it runs locally.
- **Unique both ways or nothing.** A wrong link is worse than none,
  because a link will be believed.

## Out of scope

- Line items from Willys PDFs.
- Matching letters to payments: invoices from the text of a letter
  against bankgiro and autogiro debits. The catalogue makes this
  possible, and it is the natural next step.
- Fetching. `dokument` never logs in anywhere.
