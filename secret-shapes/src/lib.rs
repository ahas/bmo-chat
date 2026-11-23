//! The secret shapes (docs/secret-shapes.md): the shapes a secret's run is written in, as the crate's
//! `shapes.toml` lists them, and the reading that replaces each such run in words by the marker its reader
//! names. What this hands back carries no value it took out, and nothing here writes a word of what it read.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Match, Regex};
use serde::Deserialize;

/// The marker a run is replaced by where a reader has no word of its own for it.
pub const MARKER: &str = "<redacted-secret>";

/// The part of a shape's run that alone is replaced, where the shape names one.
const SECRET_PART: &str = "secret";

/// What a marker reads as to a spared pattern, a letter for each of its bytes.
const MARKER_AS_LETTER: &str = "x";

// One shape as the list declares it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declared {
  name: String,
  pattern: String,
  #[serde(default)]
  spared: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
  shapes: Vec<Declared>,
}

/// One shape compiled: its name, its pattern and the runs it spares.
#[derive(Debug)]
pub struct Shape {
  name: String,
  pattern: Regex,
  spared: Vec<Regex>,
}

impl Shape {
  fn compiled(declared: Declared) -> Self {
    let compile = |pattern: &str| {
      Regex::new(pattern)
        .unwrap_or_else(|why| panic!("the secret shape {} does not compile: {why}", declared.name))
    };

    Self {
      pattern: compile(&declared.pattern),
      spared: declared.spared.iter().map(|pattern| compile(pattern)).collect(),
      name: declared.name,
    }
  }

  /// The shape's name, which says what it finds and never what it found.
  pub fn name(&self) -> &str {
    &self.name
  }

  // Whether a run lies wholly inside a run of a pattern the shape spares, read over the words as spares read them.
  fn spares(&self, spare_view: &str, run: &Match) -> bool {
    self.spared.iter().any(|spared| {
      spared
        .find_iter(spare_view)
        .any(|kept| kept.start() <= run.start() && run.end() <= kept.end())
    })
  }

  // The words with each of the shape's runs replaced by the marker, and how many, or none where it replaced none.
  fn redact(&self, text: &str, marker: &str) -> Option<(String, usize)> {
    if !self.pattern.is_match(text) {
      return None;
    }

    let spare_view = if self.spared.is_empty() { Cow::Borrowed(text) } else { spare_view(text, marker) };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut runs = 0;

    for found in self.pattern.captures_iter(text) {
      let Some(run) = found.name(SECRET_PART).or_else(|| found.get(0)) else {
        continue;
      };

      if run.is_empty() || run.as_str() == marker || self.spares(&spare_view, &run) {
        continue;
      }

      out.push_str(&text[last..run.start()]);
      out.push_str(marker);
      last = run.end();
      runs += 1;
    }

    (runs > 0).then(|| (out + &text[last..], runs))
  }
}

// The words as a spared pattern reads them: each marker as as many letters as it has bytes, so every place in
// them stands where it stands in the words.
fn spare_view<'a>(text: &'a str, marker: &str) -> Cow<'a, str> {
  if marker.is_empty() || !text.contains(marker) {
    return Cow::Borrowed(text);
  }

  Cow::Owned(text.replace(marker, &MARKER_AS_LETTER.repeat(marker.len())))
}

static SHAPES: LazyLock<Vec<Shape>> = LazyLock::new(|| {
  let list: List = toml::from_str(include_str!("../shapes.toml")).expect("the secret shapes' list reads");

  list.shapes.into_iter().map(Shape::compiled).collect()
});

/// Every shape, compiled, in the order the words are read for them.
pub fn shapes() -> &'static [Shape] {
  &SHAPES
}

/// How many runs one shape replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
  pub shape: &'static str,
  pub runs: usize,
}

/// Words read for secret-shaped runs: the words with each run replaced, borrowed where none was, and how many
/// runs each shape replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redacted<'a> {
  pub text: Cow<'a, str>,
  /// One entry to a shape that replaced a run, in the order the shapes are read.
  pub found: Vec<Found>,
}

impl Redacted<'_> {
  /// How many runs were replaced in all.
  pub fn runs(&self) -> usize {
    self.found.iter().map(|found| found.runs).sum()
  }
}

/// The words with each secret-shaped run replaced by `marker`, each shape read in turn over what the shapes
/// before it left. The marker is to be no shape's run, as `MARKER` is, so words read twice lose nothing more.
pub fn redact<'a>(text: &'a str, marker: &str) -> Redacted<'a> {
  let mut words = Cow::Borrowed(text);
  let mut found = vec![];

  for shape in shapes() {
    if let Some((replaced, runs)) = shape.redact(&words, marker) {
      words = Cow::Owned(replaced);
      found.push(Found {
        shape: shape.name(),
        runs,
      });
    }
  }

  Redacted { text: words, found }
}
