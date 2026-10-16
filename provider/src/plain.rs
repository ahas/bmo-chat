//! The plain dialect: the safe default, for a platform whose rooms take words and nothing else.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::dialect::{ChatDialect, ChatDialectLimits, ChatMarkup};

const WRITING: &str = "This room shows no formatting. Write markdown as ever, but every mark is stripped before the \
                       words go out, so nothing may rest on a mark to be understood: say it in the words themselves.";

/// The dialect of rooms that show words and nothing on them. It has no limits, does not fold, does not split,
/// never faults, and shows its markup as it stands. Its conversion strips every mark, so its markup and its
/// plain text are the same words, and a mark left standing never reaches a room as a stray character.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainDialect;

impl ChatDialect for PlainDialect {
  fn name(&self) -> &str {
    "plain"
  }

  fn limits(&self) -> ChatDialectLimits {
    ChatDialectLimits::default()
  }

  fn writing(&self) -> &str {
    WRITING
  }

  fn foldable(&self) -> bool {
    false
  }

  fn convert(&self, markdown: &str) -> ChatMarkup {
    let words = words(markdown);
    ChatMarkup {
      markup: words.clone(),
      plain: words,
      formatted: false,
    }
  }

  fn parts(&self, markdown: &str) -> Vec<String> {
    vec![markdown.to_owned()]
  }

  fn fault(&self, _markup: &str) -> Option<String> {
    None
  }

  fn fold(&self, _markdown: &str) -> Option<String> {
    None
  }

  fn visible(&self, markup: &str) -> String {
    markup.to_owned()
  }
}

/// The words of the markdown with every mark stripped, read from the parser's events. A link or a picture
/// keeps its address after its words where the two differ, a list item keeps its bullet or its number, and
/// blocks stand a blank line apart.
fn words(markdown: &str) -> String {
  let mut out = String::new();
  // The next number of each open list, or none for a bulleted one; innermost last.
  let mut lists: Vec<Option<u64>> = Vec::new();
  // Where each open link's words began, and its address.
  let mut links: Vec<(usize, String)> = Vec::new();
  // Just after an item's bullet, where the item's first block opens on the bullet's own line.
  let mut on_bullet = false;

  for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
    match event {
      Event::Start(Tag::List(first)) => {
        if !on_bullet {
          open(&mut out, if lists.is_empty() { 2 } else { 1 });
        }
        lists.push(first);
      }
      Event::End(TagEnd::List(_)) => {
        lists.pop();
      }
      Event::Start(Tag::Item) => {
        open(&mut out, 1);
        out.push_str(&"  ".repeat(lists.len().saturating_sub(1)));
        match lists.last_mut() {
          Some(Some(number)) => {
            out.push_str(&format!("{number}. "));
            *number += 1;
          }
          _ => out.push_str("- "),
        }
        on_bullet = true;
      }
      Event::Start(
        Tag::Paragraph | Tag::Heading { .. } | Tag::BlockQuote(_) | Tag::CodeBlock(_) | Tag::HtmlBlock,
      ) => {
        if !on_bullet {
          open(&mut out, 2);
        }
      }
      Event::End(TagEnd::CodeBlock) => {
        if out.ends_with('\n') {
          out.pop();
        }
      }
      Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
        links.push((out.len(), dest_url.into_string()));
      }
      Event::End(TagEnd::Link | TagEnd::Image) => {
        if let Some((start, address)) = links.pop()
          && !address.is_empty()
          && out[start..] != address
        {
          out.push_str(&format!(" ({address})"));
        }
      }
      Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
        out.push_str(&text);
        on_bullet = false;
      }
      Event::SoftBreak | Event::HardBreak => out.push('\n'),
      Event::Rule => open(&mut out, 2),
      _ => {}
    }
  }

  out.trim_end_matches('\n').to_owned()
}

/// Ends what is written so far with at least `newlines` line breaks, unless nothing is written yet.
fn open(out: &mut String, newlines: usize) {
  if out.is_empty() {
    return;
  }
  let ending = out.len() - out.trim_end_matches('\n').len();
  for _ in ending..newlines {
    out.push('\n');
  }
}
