//! The plain dialect, the base's safe default: no limits, no fold, no splitting, no faults, its markup shown
//! as it stands, and every mark stripped from the words.

use bmo_chat::{ChatDialect, ChatDialectLimits, ChatMarkup, PlainDialect};

fn words(markdown: &str) -> String {
  let read = PlainDialect.convert(markdown);
  assert_eq!(read.markup, read.plain, "markup and plain text are the same words");
  assert!(!read.formatted);
  read.plain
}

#[test]
fn what_it_declares() {
  let declaration = PlainDialect.declaration();
  assert_eq!(declaration.name, "plain");
  assert_eq!(declaration.limits, ChatDialectLimits::default());
  assert_eq!(
    declaration.writing,
    "This room shows no formatting. Write markdown as ever, but every mark is stripped before the words go \
     out, so nothing may rest on a mark to be understood: say it in the words themselves."
  );
  assert!(!declaration.foldable);
}

#[test]
fn it_never_splits_folds_or_faults_and_shows_its_markup_as_it_stands() {
  let long = "word ".repeat(10_000);
  assert_eq!(PlainDialect.parts(&long), std::slice::from_ref(&long));
  assert_eq!(PlainDialect.parts("**a**"), ["**a**"]);
  assert_eq!(PlainDialect.fold(&long), None);
  assert_eq!(PlainDialect.folded(&long), None);
  assert_eq!(PlainDialect.fault("**unclosed"), None);
  assert_eq!(PlainDialect.visible("**as it stands**"), "**as it stands**");
}

#[test]
fn a_text_asking_for_a_fold_goes_out_as_its_words() {
  assert_eq!(PlainDialect.outgoing("**bold** words", true), "bold words");
  assert_eq!(PlainDialect.outgoing("**bold** words", false), "bold words");
}

#[test]
fn inline_marks_are_stripped() {
  assert_eq!(words("**bold**, *italic*, _also_ and ~~struck~~"), "bold, italic, also and struck");
  assert_eq!(words("run `cargo test` now"), "run cargo test now");
  assert_eq!(words(r"a \*literal\* star"), "a *literal* star");
  assert_eq!(words("fish &amp; chips"), "fish & chips");
}

#[test]
fn a_link_keeps_its_address_where_its_words_differ() {
  assert_eq!(
    words("see [the docs](https://example.com/docs)"),
    "see the docs (https://example.com/docs)"
  );
  assert_eq!(words("<https://example.com>"), "https://example.com");
  assert_eq!(words("![a chart](https://example.com/c.png)"), "a chart (https://example.com/c.png)");
}

#[test]
fn blocks_stand_a_blank_line_apart() {
  assert_eq!(
    words("# Title\n\nFirst paragraph.\n\nSecond one."),
    "Title\n\nFirst paragraph.\n\nSecond one."
  );
  assert_eq!(words("one line\nthe next line"), "one line\nthe next line");
  assert_eq!(words("> quoted *words*\n\nafter"), "quoted words\n\nafter");
  assert_eq!(words("above\n\n---\n\nbelow"), "above\n\nbelow");
}

#[test]
fn a_code_block_keeps_its_text() {
  assert_eq!(
    words("before\n\n```rust\nfn main() {\n  println!(\"*\");\n}\n```\n\nafter"),
    "before\n\nfn main() {\n  println!(\"*\");\n}\n\nafter"
  );
}

#[test]
fn a_list_keeps_its_bullets_and_numbers() {
  assert_eq!(words("intro\n\n* one\n* **two**\n  - nested"), "intro\n\n- one\n- two\n  - nested");
  assert_eq!(words("3. third\n4. fourth"), "3. third\n4. fourth");
  assert_eq!(words("- loose one\n\n- loose two"), "- loose one\n- loose two");
}

#[test]
fn nothing_in_nothing_out() {
  assert_eq!(
    PlainDialect.convert(""),
    ChatMarkup {
      markup: String::new(),
      plain: String::new(),
      formatted: false
    }
  );
}
