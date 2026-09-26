# kontoutdrag

Identify merchants and categorise spending in a bank statement export.

A bank gives you one free-text field per transaction and no merchant
identity at all. `kontoutdrag` reads the export, works out what that field
is, and looks the merchant up in YAML tables you control.

```
$ kontoutdrag summary statement.csv --by category --spending -n 6
category         count      total
---------------  -----  ---------
(uncategorised)     64  -12800.00
groceries          120  -11000.00
housing             12   -9600.00
transport           31   -4650.00
utilities            8   -3200.00
restaurants         22   -2750.00
```

## Marks

The tables answer "who was paid", which is a property of the descriptor and
the same every time it appears. Some things are not like that. A week away
is a date range. A single transfer is one row on one day. Neither can be
written as a rule about a string without catching everything else that
shares it.

A mark selects transactions by date, amount and descriptor, and then sets a
category, a merchant name, or tags:

```yaml
version: 1
marks:
  - note: A week away, summer 2023
    from: 2023-06-10
    to: 2023-06-17
    tags: [resa-2023]
    descriptor_prefix: ["HOTELLET", "RESTAURANG", "MUSEET"]

  - note: A one-off transfer to savings
    date: 2024-03-15
    amount: "-25000.000"
    descriptor: ["10000000002"]
    category: savings
    merchant: Sparkontot
```

Tags sit on top of the ordinary category rather than replacing it, so a
trip totals with `--by tag` without hiding that most of it was food. A
category or merchant on a mark overrides the tables, because a mark is
written about one transaction and a rule about a whole descriptor.

The descriptor conditions are one condition between them — any listed
`descriptor` or `descriptor_prefix` matching is enough — so a trip is one
mark naming the places rather than one mark for each. Leaving them out
selects everything in the date range, which is usually not what is meant:
a subscription charged mid-trip would be swept in with it.

A mark with no selector, or with nothing to set, is an error rather than a
rule that quietly matches everything or nothing.

    [[marks]]
    path = "~/notes/finances/marks.yaml"

`kontoutdrag marks <statement>` lists them with the number of rows each one
caught, and says so when one caught nothing — almost always a typo in a
date or an amount.

## The statement side

Two formats.

**`seb`** — the CSV export described below: semicolon separated, UTF-8
with a BOM, columns
`Bokföringsdatum;Valutadatum;Verifikationsnummer;Text;Belopp;Saldo`.

**`enable-banking`** — a JSON array of transactions from Enable Banking's
account-information API, or the `{"transactions": [...]}` object the API
itself returns. Worth having because it carries the merchant name the CSV
does not: the CSV truncates a card descriptor to twelve characters, which
for a foreign purchase can leave the acquirer's city and nothing else.

A file that starts with `[` or `{` is read as JSON without being told,
since a CSV export starts with a byte-order mark or a column name.
`--format` overrides the guess.

Three things the JSON has that the CSV has no column for:
`bank_transaction_code.description` (Card purchase, Instant payment,
Mortgage, Salary/Pension/Social Benefit and so on), which is how a card
purchase is recognised once the `/YY-MM-DD` suffix is gone;
`creditor_account`, which names a transfer; and pending rows, which are
skipped — they carry no booking date and are replaced by a booked row
within a day or two.

Note that a credit transfer's descriptor is the counterparty account
followed by a payment reference — `12345678901 987654321012`. The
reference is unique per transaction, so only the account is used as the
merchant key.

### The CSV `Text` field

It comes in three shapes, which the tool tells apart:

| Shape | Meaning |
|---|---|
| `KVARNBY LIVS/26-09-09` (exactly 21 characters) | Card purchase: a merchant name truncated to twelve characters, then the date of the purchase — which is earlier than the posting date |
| All digits | A Swish counterparty: eleven digits is a phone number without the `+`, ten is a Swish-företag number |
| Anything else | Bankgiro and autogiro creditor names (24-character cap), transfers, interest, salary |

`Verifikationsnummer` is a posting-batch identifier shared by every
transaction posted in the same run, so it is not a transaction id and the
tool does not treat it as one.

## The merchant tables

A table is YAML. One ships inside the binary — `se-common`, the Swedish
chains, utilities, public bodies and subscriptions that anyone in the
country would recognise — and you add your own for everything else.

That split is deliberate. A local café, a niche web host or a
three-branch burger chain goes in your table, not this one — not only
because it is yours, but because **which** of those a bundled table
bothers to list says something about whoever built it.

The flip side matters as much, and is easier to miss: within a category
this table tries to be **complete**, listing every major player rather
than a selection. An almost-complete list leaks through its gap. Name all
six mobile operators or none of them; a list of five with the sixth
missing tells you which one the author is a customer of.

```yaml
version: 1
name: mine

merchants:
  - name: Kvarnby Livs
    category: groceries
    tags: [local, walkable]
    match:
      prefix: [KVARNBY]

  - name: Presshörnan
    category: convenience
    match:
      prefix: [PRESSHORNAN, PRESSHÖRNAN, "PH "]
      regex: ['^\d{6,8} PRESSH']
```

Descriptors are normalised before matching: upper case, and runs of
whitespace collapsed. **Diacritics are left alone.** A bank sends the same
chain as `KOPMANS TORG` and as `KÖPMANS TORG` depending on what survived
the acquirer, and folding `Ö` to `O` would cover both with one rule — but
it would also merge names that are genuinely different, and make a rule
mean something other than what it says. So write both patterns.

Four rule kinds, in decreasing order of how specific a hit counts as:

| Kind | Matches |
|---|---|
| `exact` | The whole normalised descriptor |
| `prefix` | The start of it — the workhorse, since card descriptors are truncated and so differ only at the end |
| `contains` | Anywhere in it |
| `regex` | Unanchored; anchor it yourself with `^` when you mean to |

When more than one rule matches, the most specific kind wins; within a
kind, the longer pattern wins; and if that still ties, the table loaded
last wins. That last rule is what lets your own table override the bundled
one without editing it.

**A rule can also name an amount.** Some payees bill different things
under one name: a housing association sending the monthly fee and a
parking space from the same account, say. `amount` narrows a rule to one
amount, or an inclusive range, signed as in the statement:

```yaml
- name: Parking
  category: parking
  match:
    prefix: [LANDLORD]
    amount: "-550"            # or a range: ["-600", "-500"]
```

A rule with an amount outranks every rule without one, so here the
parking rule takes the 550 kr rows and a plain `LANDLORD` rule keeps the
rest. `explain` takes `--amount` to try one out.

**A pattern ending in a space means a word boundary.** `prefix: "VT "`
matches `VT APP` but not `VTABERGSKROGEN`. YAML strips a trailing space
from an unquoted scalar, so write those patterns in quotes.

### Payment providers

A card purchase routed through a payment provider carries the provider's
name in front of the merchant's — `K*BADRUMSBOLAG`, `SP*KVARNBY`. Tables
can declare those prefixes:

```yaml
normalize:
  strip_prefixes:
    - prefix: "K*"
      provider: Klarna
```

A descriptor is looked up as written first; only if that finds nothing is
the prefix stripped and the remainder tried, so a shop whose name genuinely
starts with those letters is not mis-attributed. When the second attempt is
what worked, the provider is reported as the transaction's `via`.

## Configuration

`~/.config/skagedal-tools/kontoutdrag/settings.toml`, which
`kontoutdrag edit-config` creates and opens. Tables load in the order
written, so personal ones go at the bottom:

```toml
[[table]]
bundled = "se-common"

[[table]]
path = "~/notes/finances/merchants.yaml"

[statements]
# "seb" or "enable-banking"; a JSON file is recognised either way
format = "seb"
directory = "~/notes/finances/data"
```

With no config file at all the bundled table is used, so the tool does
something useful before it is set up. `$KONTOUTDRAG_CONFIG` overrides the
path.

## Commands

| Command | |
|---|---|
| `list <statement>` | Every transaction with the merchant and category it resolved to. `--explain` adds the table and rule that decided it, `--full` the columns the default view drops, `--unresolved` shows only what nothing matched |
| `unmatched <statement>` | The descriptors no table matched, busiest first — the work list. `--yaml` prints stubs ready to paste into a table |
| `summary <statement>` | Totals `--by category`, `merchant`, `month` or `tag` |
| `tables` | What is loaded. `--merchants` lists them all, `--bundled` lists what is compiled in, `--dump <name>` prints one to start your own from |
| `explain <descriptor>` | Look one descriptor up and see which rule decided it, and what else would have matched |
| `view <statement>...` | A window with charts to click through; see [The view](#the-view). `--json` prints the data behind it |
| `edit-config` | Open `settings.toml`, creating it from the template |

All of them take `--from` / `--to` to narrow the window, `--spending` or
`--income` to pick a direction, `--table` to add a table for one run, and
`-o table|tsv|json`.

The loop the tool is built around is: run `unmatched`, add the top few to
your table, run it again.

```
$ kontoutdrag unmatched statement.csv -n 1 --yaml
  # seen 43 times, -5200.00 kr
  - name: KVARNBY LIVS
    category: TODO
    match:
      prefix: [KVARNBY LIVS]
```

## The view

`kontoutdrag view` opens the statements in a window: spending by category
as ranked bars, the payees inside whatever is selected, spending per month,
tags, and the transactions behind it all. Click a category to see its
payees and its months; click a payee or a tag to narrow further. Several
statements can be given at once, one per account, and toggled on and off.

```
kontoutdrag view savings.json everyday.json
```

The filters above the charts scope everything below them: a period (the
last twelve full months by default), the accounts, and the categories that
move money rather than spend it — `transfer`, `income` and `refunds` are
left out of spending unless ticked back in. Spending is money out; a
refund does not net against it.

Categories are bars rather than a pie on purpose: a pie reads at five or
six slices, and a personal statement has thirty categories.

The uncategorised share of spending has its own tile, against a target of
5 %, because that bucket is the work list. Click the tile to see what is in
it; its payees are raw descriptors, which is exactly what `unmatched`
prints.

### Comments

Click a transaction in the table to open it. The box that opens takes a
comment — what the payment was for, who a number belongs to — which is
saved as you type to

    ~/.local/share/skagedal-tools/kontoutdrag/comments.json

(`$XDG_DATA_HOME` moves it, as for every tool here). To keep comments
somewhere else, a git repository say, make that file a symlink; it is
written through the link, not over it. Each comment carries the account,
date, amount, descriptor and text of its transaction, so it can be turned
into a mark or a table rule later without going back to the statement.

The opened row also lists every transaction from the day before to the day
after, in every account and regardless of the filters — where the other
half of a transfer, a repayment or a refund turns up — and links to Google
Calendar on the booking date and to Gmail for the week around it.

The view watches the settings, the statements, the tables, the marks and
the comments file, and reloads whatever changed, keeping the filters and
selection. So a rule edited elsewhere shows up in the window a couple of
seconds later, and so does a comments file that has been harvested and
cleared. If a rule file fails to load, the last good data stays up with
a banner saying why.

The window is a React app under `browser/`, embedded in the binary and
served on a local port, the same way `log-viewer` does it — the plumbing
is shared in the `webview-shell` crate. It needs the `web` feature, which
`./install` turns on; building with it runs pnpm and Vite. Without it,
`view --json` still works. The app icon, a 💰, is drawn at build time by
[appicon-generator](../appicon-generator) when it is installed, and set as
the Dock icon on macOS; it is Apple's emoji, so it is not committed, and a
build without the tool has no icon. `view --serve` prints a URL for an ordinary
browser instead of opening a window, and `?category=…`, `?merchant=…` and
`?tag=…` on that URL open on a selection. For work on the app itself,
`KONTOUTDRAG_URL=<that URL> pnpm dev` in `browser/` proxies the data.

## Limits

Foreign card purchases often arrive with nothing but a city in the
descriptor, because the merchant-name part of the authorisation message
was blank or generic and the location is what survived. Those are not
recoverable from the CSV alone — one such descriptor usually covers
several unrelated merchants, and the amounts alone will not separate
them. The tool reports them as unmatched rather than guessing.

Nothing here reads an MCC. The CSV has no column for one, and the Enable
Banking JSON has the field but a bank may leave it null. An ISO 20022
`camt.053` export carries `MrchntCtgyCd` and would beat any amount of
string matching.

The untruncated merchant name in the JSON goes most of the way instead:
the descriptors a truncated CSV cannot resolve are mostly foreign card
purchases reduced to a city.
