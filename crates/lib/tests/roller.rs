use changelog_roller_lib::{
  has_section_additions, insert_item, is_ready_to_roll, roll, RollError,
};
use orgize::{ast::Headline, Org};

/// Builds a `Vec<String>` path from string-literal segments — keeps test
/// callsites tight (`path!["Upcoming"]` instead of
/// `&vec!["Upcoming".to_string()]`).
macro_rules! path {
  ($($segment:expr),* $(,)?) => {
    vec![$($segment.to_string()),*]
  };
}

// Recursively searches the document for the first headline whose raw title
// matches `target`, returning its immediate child headlines' titles.
fn child_titles(org: &Org, parent_title: &str) -> Vec<String> {
  fn search(h: Headline, target: &str) -> Option<Headline> {
    if h.title_raw().trim() == target {
      return Some(h);
    }
    h.headlines().find_map(|c| search(c, target))
  }

  org
    .document()
    .headlines()
    .find_map(|h| search(h, parent_title))
    .map(|h| {
      h.headlines()
        .map(|c| c.title_raw().trim().to_string())
        .collect()
    })
    .unwrap_or_default()
}

// ============================================================================
// Basic structure
// ============================================================================

#[test]
fn new_upcoming_precedes_new_version() {
  let input = "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n*** Fixes\n** v0.1.0\n*** Additions\n1. Initial release\n";
  let result = roll(input.to_string(), "v0.2.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  let changelog_children = child_titles(&org, "changelog");
  assert_eq!(
    changelog_children,
    vec!["Upcoming", "v0.2.0", "v0.1.0"],
    "new Upcoming must appear before the versioned entry"
  );
}

#[test]
fn new_upcoming_carries_all_subsections() {
  let input =
    "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n*** Fixes\n";
  let result = roll(input.to_string(), "v0.1.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  assert_eq!(
    child_titles(&org, "Upcoming"),
    vec!["Breaking", "Additions", "Fixes"],
    "fresh Upcoming must contain all original subsection headings"
  );
}

#[test]
fn new_upcoming_subsections_are_empty() {
  let input = "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n1. Something added\n*** Fixes\n";
  let result = roll(input.to_string(), "v0.1.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  for title in &["Breaking", "Additions", "Fixes"] {
    // There will be two headings with these names after rolling (one in the
    // new Upcoming, one potentially in the versioned entry).  We only care
    // that the children of the new Upcoming are empty; the version entry is
    // tested separately.
    fn find(h: Headline, target: &str) -> Option<Headline> {
      if h.title_raw().trim() == target {
        return Some(h);
      }
      h.headlines().find_map(|c| find(c, target))
    }
    let upcoming = org
      .document()
      .headlines()
      .find_map(|h| find(h, "Upcoming"))
      .unwrap();
    let child = upcoming
      .headlines()
      .find(|c| c.title_raw().trim() == *title);
    assert!(child.is_some(), "new Upcoming missing subsection '{}'", title);
    assert!(
      child.unwrap().section().is_none(),
      "new Upcoming's '{}' subsection must be empty",
      title
    );
  }
}

// ============================================================================
// Empty-section pruning
// ============================================================================

#[test]
fn empty_sections_pruned_from_versioned_entry() {
  // Additions has content; Breaking and Fixes are empty.
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "*** Additions\n",
    "1. Added something new\n",
    "*** Fixes\n",
    "** v0.1.0\n",
  );
  let result = roll(input.to_string(), "v0.2.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  let version_children = child_titles(&org, "v0.2.0");
  assert_eq!(
    version_children,
    vec!["Additions"],
    "only non-empty sections should appear under the versioned entry"
  );
}

#[test]
fn all_empty_sections_yields_empty_versioned_entry() {
  let input =
    "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n*** Fixes\n";
  let result = roll(input.to_string(), "v0.1.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  let version_children = child_titles(&org, "v0.1.0");
  assert!(
    version_children.is_empty(),
    "versioned entry should have no subsections when all were empty"
  );
}

#[test]
fn all_populated_sections_all_appear_in_versioned_entry() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "1. Removed thing\n",
    "*** Additions\n",
    "1. Added thing\n",
    "*** Fixes\n",
    "1. Fixed thing\n",
  );
  let result = roll(input.to_string(), "v0.1.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  assert_eq!(
    child_titles(&org, "v0.1.0"),
    vec!["Breaking", "Additions", "Fixes"]
  );
}

// ============================================================================
// Content preservation
// ============================================================================

#[test]
fn versioned_entry_content_is_preserved() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "1. Important change\n",
  );
  let result = roll(input.to_string(), "v0.1.0", &path!["Upcoming"]).unwrap();

  // The content must appear verbatim somewhere in the output.
  assert!(
    result.contains("Important change"),
    "content from Upcoming must be preserved in the versioned entry"
  );
}

#[test]
fn pre_existing_versions_are_untouched() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "1. New thing\n",
    "** v0.2.0\n",
    "*** Additions\n",
    "1. Older thing\n",
    "** v0.1.0\n",
    "*** Additions\n",
    "1. Initial release\n",
  );
  let result = roll(input.to_string(), "v0.3.0", &path!["Upcoming"]).unwrap();
  let org = Org::parse(&result);

  assert_eq!(
    child_titles(&org, "changelog"),
    vec!["Upcoming", "v0.3.0", "v0.2.0", "v0.1.0"],
  );
  assert!(
    result.contains("Older thing"),
    "pre-existing version content must not be modified"
  );
}

// ============================================================================
// Custom upcoming heading
// ============================================================================

#[test]
fn custom_upcoming_heading_is_respected() {
  let input = "* changelog\n** Next\n*** Breaking\n*** Additions\n*** Fixes\n";
  let result = roll(input.to_string(), "v1.0.0", &path!["Next"]).unwrap();
  let org = Org::parse(&result);

  assert_eq!(child_titles(&org, "changelog"), vec!["Next", "v1.0.0"]);
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn missing_upcoming_heading_returns_error() {
  let input = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let err = roll(input.to_string(), "v0.2.0", &path!["Upcoming"]).unwrap_err();
  assert!(
    matches!(err, RollError::HeadingNotFound { .. }),
    "expected HeadingNotFound, got: {:?}",
    err
  );
}

// ============================================================================
// Ready-to-roll check
// ============================================================================

#[test]
fn ready_to_roll_true_when_any_section_has_content() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "*** Additions\n",
    "1. Something new\n",
    "*** Fixes\n",
  );
  assert!(is_ready_to_roll(input, &path!["Upcoming"]).unwrap());
}

#[test]
fn ready_to_roll_false_when_all_sections_empty() {
  let input =
    "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n*** Fixes\n";
  assert!(!is_ready_to_roll(input, &path!["Upcoming"]).unwrap());
}

#[test]
fn ready_to_roll_false_for_empty_upcoming_with_no_subsections() {
  let input = "* changelog\n** Upcoming\n";
  assert!(!is_ready_to_roll(input, &path!["Upcoming"]).unwrap());
}

#[test]
fn ready_to_roll_error_when_upcoming_heading_missing() {
  let input = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let err = is_ready_to_roll(input, &path!["Upcoming"]).unwrap_err();
  assert!(matches!(err, RollError::HeadingNotFound { .. }));
}

#[test]
fn ready_to_roll_respects_custom_upcoming_heading() {
  let input = "* changelog\n** Next\n*** Additions\n1. Something\n";
  assert!(is_ready_to_roll(input, &path!["Next"]).unwrap());
  // The default "Upcoming" name is absent — should return an error, not false.
  assert!(is_ready_to_roll(input, &path!["Upcoming"]).is_err());
}

// ============================================================================
// has_upcoming_additions
// ============================================================================

#[test]
fn diff_range_detects_new_entry() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. Brand new thing\n";
  assert!(has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_no_addition_when_head_same_as_base() {
  let input = "* changelog\n** Upcoming\n*** Additions\n1. Already there\n";
  assert!(!has_section_additions(input, input, &path!["Upcoming"]));
}

#[test]
fn diff_range_no_addition_when_head_is_subset_of_base() {
  let base =
    "* changelog\n** Upcoming\n*** Additions\n1. Already there\n1. Another\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. Already there\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_true_when_base_has_no_upcoming() {
  let base = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. New thing\n** v0.1.0\n*** Additions\n1. Initial\n";
  assert!(has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_false_when_both_have_no_upcoming() {
  let base = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let head = "* changelog\n** v0.2.0\n*** Additions\n1. Another\n** v0.1.0\n*** Additions\n1. Initial\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_headings_when_comparing() {
  // Adding a new sub-heading alone does not count as a content addition.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_comment_lines() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n# This is a comment\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_comment_heading_subtree() {
  // A "COMMENT" heading makes the whole subtree invisible.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** COMMENT Draft notes\n",
    "1. This should not count\n",
    "*** Additions\n",
  );
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_only_skips_comment_subtree_not_siblings() {
  // Content under a non-COMMENT sibling after a COMMENT subtree should count.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** COMMENT Draft notes\n",
    "1. This should not count\n",
    "*** Additions\n",
    "1. This should count\n",
  );
  assert!(has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_noexport_subtree() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Internal notes   :noexport:\n",
    "1. This should not count\n",
    "*** Additions\n",
  );
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_only_skips_noexport_subtree_not_siblings() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Internal notes   :noexport:\n",
    "1. This should not count\n",
    "*** Additions\n",
    "1. This should count\n",
  );
  assert!(has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_property_drawers() {
  // Adding a property drawer to Upcoming should not count as a visible addition.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    ":PROPERTIES:\n",
    ":CUSTOM_ID: upcoming\n",
    ":END:\n",
    "*** Additions\n",
  );
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_ignores_planning_lines() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head =
    "* changelog\n** Upcoming\nSCHEDULED: <2026-01-01>\n*** Additions\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

#[test]
fn diff_range_respects_custom_upcoming_heading() {
  let base = "* changelog\n** Next\n*** Additions\n";
  let head = "* changelog\n** Next\n*** Additions\n1. New entry\n";
  assert!(has_section_additions(base, head, &path!["Next"]));
  // "Upcoming" is absent in both — no additions under that name.
  assert!(!has_section_additions(base, head, &path!["Upcoming"]));
}

// ============================================================================
// has_section_additions drilling through nested paths
// ============================================================================

#[test]
fn diff_path_detects_addition_only_in_targeted_subsection() {
  // A new entry under Additions but not under Breaking — drilling to
  // Breaking must report no additions, while the root-only path still does.
  let base = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "*** Additions\n",
  );
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "*** Additions\n",
    "1. New thing\n",
  );
  assert!(has_section_additions(base, head, &path!["Upcoming"]));
  assert!(!has_section_additions(base, head, &path!["Upcoming", "Breaking"]));
}

#[test]
fn diff_path_detects_addition_in_drilled_subsection() {
  let base = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "*** Additions\n",
  );
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "1. Dropped foo()\n",
    "*** Additions\n",
  );
  assert!(has_section_additions(base, head, &path!["Upcoming", "Breaking"]));
}

#[test]
fn diff_path_no_additions_when_drilled_subsection_missing_in_head() {
  // If the drilled subsection does not exist in head, there is nothing
  // there to have been added — even if siblings have new content.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. New thing\n";
  assert!(!has_section_additions(base, head, &path!["Upcoming", "Breaking"]));
}

#[test]
fn diff_path_walks_multiple_segments() {
  let base =
    concat!("* changelog\n", "** Upcoming\n", "*** Breaking\n", "**** Abi\n",);
  let head = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Breaking\n",
    "**** Abi\n",
    "1. Signature change\n",
  );
  assert!(has_section_additions(
    base,
    head,
    &path!["Upcoming", "Breaking", "Abi"]
  ));
  // The signature change also surfaces under the intermediate Breaking
  // section, since visible_content recurses through sub-headlines.
  assert!(has_section_additions(base, head, &path!["Upcoming", "Breaking"]));
}

#[test]
fn diff_path_does_not_require_upcoming_as_root() {
  // The lib has no notion of "upcoming" — any heading can be the root.
  let base = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let head = concat!(
    "* changelog\n",
    "** v0.1.0\n",
    "*** Additions\n",
    "1. Initial\n",
    "1. Retroactive note\n",
  );
  assert!(has_section_additions(base, head, &path!["v0.1.0"]));
}

// ============================================================================
// insert_item
// ============================================================================

#[test]
fn insert_item_appends_next_number_to_existing_list() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "1. First thing\n",
    "*** Fixes\n",
  );
  let result = insert_item(
    input.to_string(),
    &path!["Upcoming"],
    "Additions",
    "Second thing",
  )
  .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "1. First thing\n",
      "2. Second thing\n",
      "*** Fixes\n",
    )
  );
}

#[test]
fn insert_item_starts_at_one_when_section_empty() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "*** Fixes\n",
  );
  let result = insert_item(
    input.to_string(),
    &path!["Upcoming"],
    "Additions",
    "New thing",
  )
  .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "1. New thing\n",
      "*** Fixes\n",
    )
  );
}

#[test]
fn insert_item_creates_missing_subheading() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "** v0.1.0\n",
  );
  let result = insert_item(
    input.to_string(),
    &path!["Upcoming"],
    "Breaking",
    "Removed shiny",
  )
  .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "*** Breaking\n",
      "1. Removed shiny\n",
      "** v0.1.0\n",
    )
  );
}

#[test]
fn insert_item_creates_subheading_at_eof_when_upcoming_is_last_section() {
  let input = "* changelog\n** Upcoming\n*** Additions\n";
  let result =
    insert_item(input.to_string(), &path!["Upcoming"], "Fixes", "Fixed thing")
      .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "*** Fixes\n",
      "1. Fixed thing\n",
    )
  );
}

#[test]
fn insert_item_preserves_blank_line_separator_when_creating_subheading() {
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "1. Already here\n",
    "\n",
    "** v0.1.0\n",
  );
  let result =
    insert_item(input.to_string(), &path!["Upcoming"], "Fixes", "Fixed thing")
      .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "1. Already here\n",
      "*** Fixes\n",
      "1. Fixed thing\n",
      "\n",
      "** v0.1.0\n",
    )
  );
}

#[test]
fn insert_item_returns_error_when_upcoming_missing() {
  let input = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let err =
    insert_item(input.to_string(), &path!["Upcoming"], "Additions", "Body")
      .unwrap_err();
  assert!(
    matches!(err, RollError::HeadingNotFound { .. }),
    "expected HeadingNotFound, got: {:?}",
    err
  );
}

#[test]
fn insert_item_does_not_touch_versioned_section_with_same_name() {
  // Both Upcoming and v0.1.0 have an "Additions" subheading.  Inserting
  // into "Additions" must target the one under Upcoming.
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "** v0.1.0\n",
    "*** Additions\n",
    "1. Initial release\n",
  );
  let result = insert_item(
    input.to_string(),
    &path!["Upcoming"],
    "Additions",
    "New thing",
  )
  .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "1. New thing\n",
      "** v0.1.0\n",
      "*** Additions\n",
      "1. Initial release\n",
    )
  );
}

#[test]
fn insert_item_heading_match_is_exact() {
  // "Fix" must not match "Fixes" — they are different headings, so a new
  // "Fix" subheading should be created.
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Fixes\n",
    "1. Fixed something\n",
  );
  let result = insert_item(
    input.to_string(),
    &path!["Upcoming"],
    "Fix",
    "Different category",
  )
  .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Fixes\n",
      "1. Fixed something\n",
      "*** Fix\n",
      "1. Different category\n",
    )
  );
}

#[test]
fn insert_item_does_not_renumber_existing_items() {
  // If the existing list has unusual numbering, we still just use max+1.
  let input = concat!(
    "* changelog\n",
    "** Upcoming\n",
    "*** Additions\n",
    "1. First\n",
    "1. Second (typo)\n",
    "1. Third (typo)\n",
  );
  let result =
    insert_item(input.to_string(), &path!["Upcoming"], "Additions", "Fourth")
      .unwrap();
  // max numbered value is 1, so new item is 2 — and the typos are left as-is.
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Upcoming\n",
      "*** Additions\n",
      "1. First\n",
      "1. Second (typo)\n",
      "1. Third (typo)\n",
      "2. Fourth\n",
    )
  );
}

#[test]
fn insert_item_respects_custom_upcoming_heading() {
  let input =
    concat!("* changelog\n", "** Next\n", "*** Additions\n", "** v0.1.0\n",);
  let result =
    insert_item(input.to_string(), &path!["Next"], "Additions", "New thing")
      .unwrap();
  assert_eq!(
    result,
    concat!(
      "* changelog\n",
      "** Next\n",
      "*** Additions\n",
      "1. New thing\n",
      "** v0.1.0\n",
    )
  );
}
