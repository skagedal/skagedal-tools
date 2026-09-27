//! End-to-end tests: the real binary, a statement on disk, and tables
//! pointed at by a real config file.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A made-up statement. The two chains are real, because the point is to
/// exercise the real bundled table; everything else — the amounts, the
/// balances, the posting batches, the Swish number, the local shop — is
/// invented, and the balance column is internally consistent so the
/// fixture reads like a real export without being one.
const STATEMENT: &str = "\u{feff}Bokföringsdatum;Valutadatum;Verifikationsnummer;Text;Belopp;Saldo
2026-09-11;2026-09-11;1000000001;PRESSBYRAN  /26-09-10;-30.000;10000.000
2026-09-11;2026-09-11;1000000001;ICA SUPERMAR/26-09-10;-450.000;10030.000
2026-09-10;2026-09-10;1000000002;46700000001;-200.000;10480.000
2026-09-10;2026-09-10;1000000003;KVARNBY LIVS/26-09-09;-100.000;10680.000
2026-08-02;2026-08-02;1000000004;CLAS OHLSON /26-08-01;-250.000;10780.000
2026-08-01;2026-08-01;1000000005;LÖN;5000.000;11030.000
";

const PERSONAL_TABLE: &str = r#"
version: 1
name: personal
merchants:
  - name: Kvarnby Livs
    category: food/groceries
    tags: [local]
    match:
      prefix: [KVARNBY]
  - name: Salary
    category: income
    match:
      exact: ["LÖN"]
"#;

struct Fixture {
    _dir: tempfile::TempDir,
    statement: PathBuf,
    config: PathBuf,
}

fn fixture(config_body: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let statement = dir.path().join("kontoutdrag.csv");
    std::fs::write(&statement, STATEMENT).unwrap();

    let table = dir.path().join("personal.yaml");
    std::fs::write(&table, PERSONAL_TABLE).unwrap();

    let config = dir.path().join("settings.toml");
    let body = config_body.replace("{TABLE}", table.to_str().unwrap());
    std::fs::write(&config, body).unwrap();

    Fixture {
        _dir: dir,
        statement,
        config,
    }
}

fn run(config: &Path, args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_kontoutdrag"))
        .args(args)
        .env("KONTOUTDRAG_CONFIG", config)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn resolves_bundled_and_personal_tables_together() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, stderr, ok) = run(
        &f.config,
        &["list", f.statement.to_str().unwrap(), "--explain"],
    );
    assert!(ok, "{stderr}");

    // From the bundled table.
    assert!(stdout.contains("Pressbyrån"), "{stdout}");
    assert!(stdout.contains("Clas Ohlson"), "{stdout}");
    // From the personal one.
    assert!(stdout.contains("Kvarnby Livs"), "{stdout}");
    assert!(stdout.contains("personal:"), "{stdout}");
    // Swish numbers have no merchant, and are not invented.
    assert!(stdout.contains("46700000001"), "{stdout}");
}

#[test]
fn unmatched_lists_only_what_no_table_knew() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, stderr, ok) = run(&f.config, &["unmatched", f.statement.to_str().unwrap()]);
    assert!(ok, "{stderr}");

    assert!(stdout.contains("46700000001"), "{stdout}");
    assert!(!stdout.contains("PRESSBYRAN"), "{stdout}");
    assert!(!stdout.contains("KVARNBY"), "{stdout}");
    assert!(stderr.contains("1 of 6 transactions unmatched"), "{stderr}");
}

#[test]
fn unmatched_yaml_stubs_paste_into_a_table() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, _, ok) = run(
        &f.config,
        &["unmatched", f.statement.to_str().unwrap(), "--yaml"],
    );
    assert!(ok);
    assert!(stdout.contains("category: TODO"), "{stdout}");

    // The point of the stub is that it can be pasted into a table, so
    // check it actually loads rather than that it looks a certain way.
    let dir = tempfile::tempdir().unwrap();
    let table = dir.path().join("pasted.yaml");
    std::fs::write(
        &table,
        format!("version: 1\nname: pasted\nmerchants:\n{stdout}").replace("TODO", "todo"),
    )
    .unwrap();
    let config = dir.path().join("settings.toml");
    std::fs::write(&config, "").unwrap();

    let (explained, stderr, ok) = run(
        &config,
        &[
            "explain",
            "46700000001",
            "--only-tables",
            "--table",
            table.to_str().unwrap(),
        ],
    );
    assert!(ok, "the pasted stub did not load: {stderr}");
    assert!(explained.contains("merchant    46700000001"), "{explained}");
}

#[test]
fn summary_totals_by_category() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, stderr, ok) = run(
        &f.config,
        &[
            "summary",
            f.statement.to_str().unwrap(),
            "--by",
            "category",
            "-o",
            "tsv",
        ],
    );
    assert!(ok, "{stderr}");

    let row = |name: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(&format!("{name}\t")))
            .unwrap_or_else(|| panic!("no {name} row in\n{stdout}"))
            .to_string()
    };
    // ICA 450.00 + Kvarnby 100.00
    assert_eq!(row("food/groceries"), "food/groceries\t2\t-550.00");
    assert_eq!(row("income"), "income\t1\t5000.00");
    // The Swish payment resolved to nothing.
    assert_eq!(row("(uncategorised)"), "(uncategorised)\t1\t-200.00");
}

#[test]
fn a_later_table_overrides_the_bundled_one() {
    let dir = tempfile::tempdir().unwrap();
    let statement = dir.path().join("s.csv");
    std::fs::write(&statement, STATEMENT).unwrap();
    let table = dir.path().join("override.yaml");
    std::fs::write(
        &table,
        "version: 1\nname: mine\nmerchants:\n  - name: The kiosk by the station\n    category: snacks\n    match:\n      prefix: [PRESSBYRAN]\n",
    )
    .unwrap();
    let config = dir.path().join("settings.toml");
    std::fs::write(
        &config,
        format!(
            "[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{}\"\n",
            table.to_str().unwrap()
        ),
    )
    .unwrap();

    let (stdout, _, ok) = run(&config, &["list", statement.to_str().unwrap()]);
    assert!(ok);
    assert!(stdout.contains("The kiosk by the station"), "{stdout}");
    assert!(!stdout.contains("Pressbyrån"), "{stdout}");
}

#[test]
fn date_and_direction_filters_narrow_the_window() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, _, ok) = run(
        &f.config,
        &[
            "list",
            f.statement.to_str().unwrap(),
            "--from",
            "2026-09-01",
            "--spending",
            "-o",
            "tsv",
        ],
    );
    assert!(ok);
    assert_eq!(stdout.lines().count(), 5, "header plus four rows\n{stdout}");
    assert!(!stdout.contains("LÖN"), "{stdout}");
    assert!(!stdout.contains("2026-08"), "{stdout}");
}

#[test]
fn works_with_no_config_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let statement = dir.path().join("s.csv");
    std::fs::write(&statement, STATEMENT).unwrap();
    let missing = dir.path().join("nope.toml");

    let (stdout, stderr, ok) = run(&missing, &["list", statement.to_str().unwrap()]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("Pressbyrån"), "{stdout}");
}

#[test]
fn a_file_that_is_not_a_statement_is_an_error_not_an_empty_report() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.csv");
    std::fs::write(&junk, "a,b\n1,2\n").unwrap();
    let config = dir.path().join("settings.toml");
    std::fs::write(&config, "").unwrap();

    let (_, stderr, ok) = run(&config, &["list", junk.to_str().unwrap()]);
    assert!(!ok);
    assert!(stderr.contains("does not look like"), "{stderr}");
}

#[test]
fn explain_names_the_table_and_rule() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, _, ok) = run(&f.config, &["explain", "ICA SUPERMAR/26-09-10"]);
    assert!(ok);
    assert!(stdout.contains("kind        card"), "{stdout}");
    assert!(stdout.contains("merchant    ICA"), "{stdout}");
    assert!(stdout.contains("se-common"), "{stdout}");
}

#[test]
fn view_json_carries_every_transaction_resolved() {
    let f = fixture("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n");
    let (stdout, stderr, ok) = run(
        &f.config,
        &["view", "--json", f.statement.to_str().unwrap()],
    );
    assert!(ok, "{stderr}");
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(data["accounts"], serde_json::json!(["kontoutdrag"]));
    let rows = data["transactions"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    let kvarnby = rows
        .iter()
        .find(|r| r["merchant"] == "Kvarnby Livs")
        .unwrap();
    assert_eq!(kvarnby["category"], "food/groceries");
    assert_eq!(kvarnby["tags"], serde_json::json!(["local"]));
    assert_eq!(kvarnby["amount"], -100.0);
    // A CSV has no bank reference, so the key is built from the row itself.
    assert_eq!(
        kvarnby["key"],
        "kontoutdrag|2026-09-10|-100.00|KVARNBY LIVS/26-09-09"
    );
    let swish = rows.iter().find(|r| r["kind"] == "swish").unwrap();
    assert_eq!(swish["resolved"], false);
}

fn budget_fixture() -> Fixture {
    let f = fixture(
        "[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"{TABLE}\"\n\n[budgets]\npath = \"{BUDGETS}\"\n",
    );
    let dir = f.config.parent().unwrap().join("budget");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        dir.join("2026-09.yaml"),
        "version: 1\nmonth: 2026-09\nincome:\n  - {name: Salary, category: income, amount: 5000}\nrows:\n  - {name: Groceries, category: food/groceries, amount: 1000}\n",
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not a budget").unwrap();
    let body = std::fs::read_to_string(&f.config)
        .unwrap()
        .replace("{BUDGETS}", dir.to_str().unwrap());
    std::fs::write(&f.config, body).unwrap();
    f
}

#[test]
fn budget_compares_a_month_with_its_file() {
    let f = budget_fixture();
    let (stdout, stderr, ok) = run(
        &f.config,
        &[
            "budget",
            f.statement.to_str().unwrap(),
            "--month",
            "2026-09",
            "-o",
            "tsv",
        ],
    );
    assert!(ok, "{stderr}");
    let row = |name: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(&format!("{name}\t")))
            .unwrap_or_else(|| panic!("no {name} row in\n{stdout}"))
            .to_string()
    };
    // ICA 450 + Kvarnby 100 in September; the salary came in August.
    assert_eq!(
        row("Groceries"),
        "Groceries\tfood/groceries\t1000.00\t550.00\t450.00"
    );
    assert_eq!(row("Salary"), "Salary\tincome\t5000.00\t0.00\t5000.00");
    // The kiosk and the Swish payment.
    assert_eq!(row("(unbudgeted)"), "(unbudgeted)\t\t\t230.00\t");

    let (stdout, stderr, ok) = run(
        &f.config,
        &[
            "budget",
            f.statement.to_str().unwrap(),
            "--month",
            "2026-09",
            "--unbudgeted",
            "-o",
            "tsv",
        ],
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout.lines().count(), 3, "{stdout}");
    assert!(stdout.contains("46700000001"), "{stdout}");
}

#[test]
fn view_json_carries_the_budgets() {
    let f = budget_fixture();
    let (stdout, stderr, ok) = run(
        &f.config,
        &["view", "--json", f.statement.to_str().unwrap()],
    );
    assert!(ok, "{stderr}");
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let months = data["budgets"]["months"].as_array().unwrap();
    assert_eq!(months.len(), 1);
    let groceries = &months[0]["rows"][0];
    assert_eq!(groceries["amount"], 1000.0);
    assert_eq!(groceries["actual"], 550.0);
    assert_eq!(groceries["keys"].as_array().unwrap().len(), 2);
    assert_eq!(months[0]["unbudgeted"]["actual"], 230.0);
}
