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

## The statement side

Currently one format: `seb`, the CSV that SEB internetbanken writes from
"Spara kontohändelser" — semicolon separated, UTF-8 with a BOM, columns
`Bokföringsdatum;Valutadatum;Verifikationsnummer;Text;Belopp;Saldo`.

The `Text` field comes in three shapes, which the tool tells apart:

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

## Limits

Foreign card purchases often arrive with nothing but a city in the
descriptor, because the merchant-name part of the authorisation message
was blank or generic and the location is what survived. Those are not
recoverable from the CSV alone — one such descriptor usually covers
several unrelated merchants, and the amounts alone will not separate
them. The tool reports them as unmatched rather than guessing.

Nothing here reads an MCC, because the CSV has no column for one. A bank's
PSD2 API or an ISO 20022 `camt.053` export can carry `MrchntCtgyCd`, which
would beat any amount of string matching; that would be a second statement
format rather than a change to the tables.
