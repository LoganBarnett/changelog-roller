use std::collections::HashSet;

use orgize::{
  ast::{Headline, List},
  rowan::{ast::AstNode, NodeOrToken, TextRange, TextSize},
  Org, SyntaxKind, SyntaxNode,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RollError {
  #[error("No '{heading}' heading found in changelog")]
  HeadingNotFound { heading: String },
}

/// Returns one entry per piece of "visible content" found beneath `node`:
/// list items and prose paragraphs.  Structural nodes that orgize parses
/// as their own kinds — property drawers, planning lines, keyword
/// metadata, comments, blank lines — never match an arm here, so they
/// fall through silently.  Sub-headlines carrying a `COMMENT` keyword or
/// a `:noexport:` tag are skipped wholesale; other sub-headlines are
/// recursed into.
fn visible_content(node: &SyntaxNode) -> Vec<String> {
  node
    .children()
    .flat_map(|child| match child.kind() {
      SyntaxKind::HEADLINE => Headline::cast(child.clone())
        .filter(|h| !h.is_commented() && !h.tags().any(|t| t == "noexport"))
        .map(|_| visible_content(&child))
        .unwrap_or_default(),
      SyntaxKind::SECTION => visible_content(&child),
      SyntaxKind::LIST => child
        .children()
        .filter(|c| c.kind() == SyntaxKind::LIST_ITEM)
        .map(|item| item.to_string().trim_end().to_string())
        .collect(),
      SyntaxKind::PARAGRAPH => {
        vec![child.to_string().trim_end().to_string()]
      }
      _ => Vec::new(),
    })
    .filter(|s| !s.is_empty())
    .collect()
}

/// Returns the visible content entries under the section reached by
/// walking `path`, or an empty `Vec` when any segment of that path is
/// absent — so callers can still diff even if one side has no such
/// section at all.
fn section_visible_content(org_text: &str, path: &[String]) -> Vec<String> {
  let org = Org::parse(org_text);
  find_section_at_path(&org, path)
    .map(|h| visible_content(h.syntax()))
    .unwrap_or_default()
}

/// Returns `true` if `head_text` contains visible-content entries under
/// the section at `path` that are not present in `base_text`.  `path`'s
/// first segment is searched anywhere in the document; subsequent
/// segments drill into child headings by exact title match.  By
/// convention the first segment is the configured "upcoming" heading,
/// but nothing in this function depends on that — any starting heading
/// works.
pub fn has_section_additions(
  base_text: &str,
  head_text: &str,
  path: &[String],
) -> bool {
  let base: HashSet<String> = section_visible_content(base_text, path)
    .into_iter()
    .collect();
  section_visible_content(head_text, path)
    .iter()
    .any(|l| !base.contains(l))
}

/// Locates the section identified by `path`.  The first segment is
/// searched anywhere in the document by exact title match; subsequent
/// segments drill into direct child headings by exact title match.
/// Returns `HeadingNotFound` carrying the segment that failed to
/// resolve.  An empty path is itself a programmer-side `HeadingNotFound`
/// — callers always pass at least the root segment.
fn find_section_at_path(
  org: &Org,
  path: &[String],
) -> Result<Headline, RollError> {
  fn search(headline: &Headline, target: &str) -> Option<Headline> {
    if headline.title_raw().trim() == target {
      return Some(headline.clone());
    }
    headline
      .headlines()
      .find_map(|child| search(&child, target))
  }

  let (root, rest) =
    path
      .split_first()
      .ok_or_else(|| RollError::HeadingNotFound {
        heading: String::new(),
      })?;

  let root_headline = org
    .document()
    .headlines()
    .find_map(|top| search(&top, root))
    .ok_or_else(|| RollError::HeadingNotFound {
      heading: root.clone(),
    })?;

  rest.iter().try_fold(root_headline, |current, segment| {
    current
      .headlines()
      .find(|child| child.title_raw().trim() == segment.as_str())
      .ok_or_else(|| RollError::HeadingNotFound {
        heading: segment.clone(),
      })
  })
}

/// Returns `true` if the section at `path` has at least one direct child
/// heading with content, indicating the changelog is ready to be stamped
/// with a version.  Returns `false` if every child heading is empty.
///
/// Intended for use as a CI gate: a non-ready result should produce a
/// non-zero exit code so pull requests without documented changes are
/// blocked from releasing.  The conventional `path` is `["Upcoming"]`,
/// but anything that resolves to a section works.
pub fn is_ready_to_roll(
  org_text: &str,
  path: &[String],
) -> Result<bool, RollError> {
  let org = Org::parse(org_text);
  let section = find_section_at_path(&org, path)?;
  Ok(section.headlines().any(|h| h.section().is_some()))
}

/// Rolls a changelog forward by stamping the section at `path` as a new
/// version and inserting a fresh empty replica above it.
///
/// Empty subsections (those with no content) are pruned from the versioned
/// entry so that only changes that actually happened appear under the new
/// version.  The fresh replica always carries the full set of subsection
/// headings, ready to be populated.  The conventional `path` is
/// `["Upcoming"]`; the lib itself has no opinion.
pub fn roll(
  org_text: String,
  new_version: &str,
  path: &[String],
) -> Result<String, RollError> {
  let mut org = Org::parse(&org_text);
  let upcoming = find_section_at_path(&org, path)?;
  let upcoming_title = upcoming.title_raw().trim_end().to_string();

  let stars = "*".repeat(upcoming.level());
  let substars = "*".repeat(upcoming.level() + 1);

  let subheadings: Vec<Headline> = upcoming.headlines().collect();

  let mut fresh = format!("{} {}\n", stars, upcoming_title);
  for sub in &subheadings {
    fresh.push_str(&format!("{} {}\n", substars, sub.title_raw().trim_end()));
  }

  let mut versioned = format!("{} {}\n", stars, new_version);
  for sub in &subheadings {
    if sub.section().is_some() {
      versioned.push_str(&sub.syntax().to_string());
    }
  }

  let range = upcoming.syntax().text_range();
  org.replace_range(range, format!("{}{}", fresh, versioned));

  Ok(org.to_org())
}

/// Returns the end position of the last non-blank-line token within
/// `node`'s subtree.  In particular this excludes trailing `BLANK_LINE`
/// tokens but keeps the terminating `NEW_LINE` of a real content line —
/// so inserting at the returned offset places new content immediately
/// after the last real line and before any blank-line separator.
fn last_content_end(node: &SyntaxNode) -> TextSize {
  node
    .descendants_with_tokens()
    .filter_map(|nt| match nt {
      NodeOrToken::Token(t) if t.kind() != SyntaxKind::BLANK_LINE => {
        Some(t.text_range().end())
      }
      _ => None,
    })
    .last()
    .unwrap_or_else(|| node.text_range().end())
}

/// Parses the leading numeric portion of an ordered-list bullet such as
/// `"1. "`, `"42. "`, or `"3) "`.  Returns `None` for unordered bullets
/// (`"- "`, `"+ "`, `"* "`) so a `filter_map` over mixed bullets yields
/// only the numeric ones.
fn bullet_number(bullet: &str) -> Option<u32> {
  let digits: String =
    bullet.chars().take_while(|c| c.is_ascii_digit()).collect();
  digits.parse::<u32>().ok()
}

/// Inserts a new ordered-list item under a subheading of the section
/// identified by `parent_path`.  The item is appended after the last
/// existing numbered entry, numbered as `<max + 1>`; if no numbered
/// entries exist yet the new item is numbered `1`.  If `item_heading`
/// does not exist as a direct child of the parent, it is created at the
/// end of the parent's span with the new item as its only content.
///
/// Heading matching is exact: `item_heading` must equal the subheading's
/// raw title byte-for-byte (after trimming the trailing whitespace orgize
/// includes on titles).  Only subheadings directly under the parent are
/// considered; identically-named subheadings under sibling sections are
/// left alone.  The conventional `parent_path` is `["Upcoming"]`.
pub fn insert_item(
  org_text: String,
  parent_path: &[String],
  item_heading: &str,
  body: &str,
) -> Result<String, RollError> {
  let mut org = Org::parse(&org_text);
  let upcoming = find_section_at_path(&org, parent_path)?;

  let existing = upcoming
    .headlines()
    .find(|h| h.title_raw().trim() == item_heading);

  match existing {
    Some(sub) => {
      let list = sub
        .section()
        .and_then(|s| s.syntax().children().find_map(List::cast));

      let (insert_offset, next_n) = match list {
        Some(list) => {
          let max_n = list
            .items()
            .filter_map(|item| bullet_number(item.bullet().as_ref()))
            .max()
            .unwrap_or(0);
          (list.syntax().text_range().end(), max_n + 1)
        }
        None => (sub.syntax().text_range().end(), 1),
      };

      let new_item = format!("{}. {}\n", next_n, body);
      org
        .replace_range(TextRange::new(insert_offset, insert_offset), &new_item);
    }
    None => {
      let substars = "*".repeat(upcoming.level() + 1);
      let new_block = format!("{} {}\n1. {}\n", substars, item_heading, body);

      // If upcoming already has sub-headlines, replace the last one's full
      // text with (its content up to the last non-blank line + the new
      // sub-headline block + its trailing blank lines).  Splicing inside
      // a single headline triggers orgize's per-headline reparse, which
      // can lose sibling content; expanding the range to cover the whole
      // sub-headline forces the multi-headline full-document reparse path.
      let subs: Vec<Headline> = upcoming.headlines().collect();
      match subs.last() {
        Some(last) => {
          let last_range = last.syntax().text_range();
          let last_text = last.syntax().to_string();
          let split_at: usize =
            (last_content_end(last.syntax()) - last_range.start()).into();
          let mut new_text = last_text[..split_at].to_string();
          new_text.push_str(&new_block);
          new_text.push_str(&last_text[split_at..]);
          org.replace_range(last_range, new_text);
        }
        None => {
          let end = upcoming.syntax().text_range().end();
          org.replace_range(TextRange::new(end, end), &new_block);
        }
      }
    }
  }

  Ok(org.to_org())
}
