use orgize::{elements::Title, Headline, Org};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RollError {
  #[error("No '{heading}' heading found in changelog")]
  UpcomingNotFound { heading: String },

  #[error("Failed to modify changelog structure: {0}")]
  StructureModification(String),

  #[error("Failed to write rolled changelog output: {0}")]
  OutputWrite(#[from] std::io::Error),

  #[error("Rolled changelog output is not valid UTF-8: {0}")]
  OutputEncoding(#[from] std::string::FromUtf8Error),
}

/// Returns `true` if the upcoming section has at least one subsection with
/// content, indicating the changelog is ready to be stamped with a version.
/// Returns `false` if every subsection is empty.
///
/// Intended for use as a CI gate: a non-ready result should produce a
/// non-zero exit code so pull requests without documented changes are
/// blocked from releasing.
pub fn is_ready_to_roll(
  org_text: &str,
  upcoming_heading: &str,
) -> Result<bool, RollError> {
  let org = Org::parse(org_text);

  let upcoming = org
    .headlines()
    .find(|h| h.title(&org).raw.as_ref() == upcoming_heading)
    .ok_or_else(|| RollError::UpcomingNotFound {
      heading: upcoming_heading.to_string(),
    })?;

  let ready = upcoming.children(&org).any(|c| c.section_node().is_some());
  Ok(ready)
}

/// Rolls a changelog forward by stamping the upcoming section as a new
/// version and inserting a fresh empty upcoming section above it.
///
/// Empty subsections (those with no content) are pruned from the versioned
/// entry so that only changes that actually happened appear under the new
/// version.  The fresh upcoming section always carries the full set of
/// subsection headings, ready to be populated.
pub fn roll(
  org_text: String,
  new_version: &str,
  upcoming_heading: &str,
) -> Result<String, RollError> {
  let mut org = Org::parse_string(org_text);

  let upcoming = org
    .headlines()
    .find(|h| h.title(&org).raw.as_ref() == upcoming_heading)
    .ok_or_else(|| RollError::UpcomingNotFound {
      heading: upcoming_heading.to_string(),
    })?;

  let upcoming_level = upcoming.level();

  // Collect children before any mutation so borrows of `org` don't overlap
  // with the mutable borrows required for tree surgery below.
  let children: Vec<Headline> = upcoming.children(&org).collect();
  let child_titles: Vec<String> = children
    .iter()
    .map(|c| c.title(&org).raw.to_string())
    .collect();
  let child_has_content: Vec<bool> = children
    .iter()
    .map(|c| c.section_node().is_some())
    .collect();

  // Detach every child so they can be rehomed.
  for &child in &children {
    child.detach(&mut org);
  }

  // Build the versioned heading; non-empty children carry their content into
  // it.  Empty children are intentionally discarded — no empty sections in
  // released versions.
  let new_version_hl = Headline::new(
    Title {
      level: upcoming_level,
      raw: String::from(new_version).into(),
      ..Title::default()
    },
    &mut org,
  );
  for (i, &child) in children.iter().enumerate() {
    if child_has_content[i] {
      new_version_hl
        .append(child, &mut org)
        .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
    }
  }

  // Build the fresh upcoming heading with all subsections recreated empty.
  let new_upcoming_hl = Headline::new(
    Title {
      level: upcoming_level,
      raw: String::from(upcoming_heading).into(),
      ..Title::default()
    },
    &mut org,
  );
  for title in &child_titles {
    let empty_child = Headline::new(
      Title {
        level: upcoming_level + 1,
        raw: title.clone().into(),
        ..Title::default()
      },
      &mut org,
    );
    new_upcoming_hl
      .append(empty_child, &mut org)
      .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  }

  // Splice both new headings into the document at the old upcoming's position,
  // then remove the now-empty old upcoming.
  upcoming
    .insert_before(new_upcoming_hl, &mut org)
    .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  new_upcoming_hl
    .insert_after(new_version_hl, &mut org)
    .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  upcoming.detach(&mut org);

  let mut output: Vec<u8> = Vec::new();
  org.write_org(&mut output)?;
  Ok(String::from_utf8(output)?)
}
