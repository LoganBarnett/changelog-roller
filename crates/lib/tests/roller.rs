use changelog_roller_lib::{
  has_upcoming_additions, is_ready_to_roll, roll, RollError,
};
use orgize::Org;

// Returns the immediate child headline titles for the first headline whose
// title matches `parent_title`.
fn child_titles(org: &Org, parent_title: &str) -> Vec<String> {
  org
    .headlines()
    .find(|h| h.title(org).raw.as_ref() == parent_title)
    .map(|h| {
      h.children(org)
        .map(|c| c.title(org).raw.to_string())
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
  let result = roll(input.to_string(), "v0.2.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v0.1.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v0.1.0", "Upcoming").unwrap();
  let org = Org::parse(&result);

  for title in &["Breaking", "Additions", "Fixes"] {
    // There will be two headings with these names after rolling (one in the
    // new Upcoming, one potentially in the versioned entry).  We only care
    // that the children of the new Upcoming are empty; the version entry is
    // tested separately.
    let upcoming = org
      .headlines()
      .find(|h| h.title(&org).raw.as_ref() == "Upcoming")
      .unwrap();
    let child = upcoming
      .children(&org)
      .find(|c| c.title(&org).raw.as_ref() == *title);
    assert!(child.is_some(), "new Upcoming missing subsection '{}'", title);
    assert!(
      child.unwrap().section_node().is_none(),
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
  let result = roll(input.to_string(), "v0.2.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v0.1.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v0.1.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v0.1.0", "Upcoming").unwrap();

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
  let result = roll(input.to_string(), "v0.3.0", "Upcoming").unwrap();
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
  let result = roll(input.to_string(), "v1.0.0", "Next").unwrap();
  let org = Org::parse(&result);

  assert_eq!(child_titles(&org, "changelog"), vec!["Next", "v1.0.0"]);
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn missing_upcoming_heading_returns_error() {
  let input = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let err = roll(input.to_string(), "v0.2.0", "Upcoming").unwrap_err();
  assert!(
    matches!(err, RollError::UpcomingNotFound { .. }),
    "expected UpcomingNotFound, got: {:?}",
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
  assert!(is_ready_to_roll(input, "Upcoming").unwrap());
}

#[test]
fn ready_to_roll_false_when_all_sections_empty() {
  let input =
    "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n*** Fixes\n";
  assert!(!is_ready_to_roll(input, "Upcoming").unwrap());
}

#[test]
fn ready_to_roll_false_for_empty_upcoming_with_no_subsections() {
  let input = "* changelog\n** Upcoming\n";
  assert!(!is_ready_to_roll(input, "Upcoming").unwrap());
}

#[test]
fn ready_to_roll_error_when_upcoming_heading_missing() {
  let input = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let err = is_ready_to_roll(input, "Upcoming").unwrap_err();
  assert!(matches!(err, RollError::UpcomingNotFound { .. }));
}

#[test]
fn ready_to_roll_respects_custom_upcoming_heading() {
  let input = "* changelog\n** Next\n*** Additions\n1. Something\n";
  assert!(is_ready_to_roll(input, "Next").unwrap());
  // The default "Upcoming" name is absent — should return an error, not false.
  assert!(is_ready_to_roll(input, "Upcoming").is_err());
}

// ============================================================================
// has_upcoming_additions
// ============================================================================

#[test]
fn diff_range_detects_new_entry() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. Brand new thing\n";
  assert!(has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_no_addition_when_head_same_as_base() {
  let input = "* changelog\n** Upcoming\n*** Additions\n1. Already there\n";
  assert!(!has_upcoming_additions(input, input, "Upcoming"));
}

#[test]
fn diff_range_no_addition_when_head_is_subset_of_base() {
  let base =
    "* changelog\n** Upcoming\n*** Additions\n1. Already there\n1. Another\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. Already there\n";
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_true_when_base_has_no_upcoming() {
  let base = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n1. New thing\n** v0.1.0\n*** Additions\n1. Initial\n";
  assert!(has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_false_when_both_have_no_upcoming() {
  let base = "* changelog\n** v0.1.0\n*** Additions\n1. Initial\n";
  let head = "* changelog\n** v0.2.0\n*** Additions\n1. Another\n** v0.1.0\n*** Additions\n1. Initial\n";
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_ignores_headings_when_comparing() {
  // Adding a new sub-heading alone does not count as a content addition.
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Breaking\n*** Additions\n";
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_ignores_comment_lines() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head = "* changelog\n** Upcoming\n*** Additions\n# This is a comment\n";
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
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
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
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
  assert!(has_upcoming_additions(base, head, "Upcoming"));
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
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
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
  assert!(has_upcoming_additions(base, head, "Upcoming"));
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
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_ignores_planning_lines() {
  let base = "* changelog\n** Upcoming\n*** Additions\n";
  let head =
    "* changelog\n** Upcoming\nSCHEDULED: <2026-01-01>\n*** Additions\n";
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}

#[test]
fn diff_range_respects_custom_upcoming_heading() {
  let base = "* changelog\n** Next\n*** Additions\n";
  let head = "* changelog\n** Next\n*** Additions\n1. New entry\n";
  assert!(has_upcoming_additions(base, head, "Next"));
  // "Upcoming" is absent in both — no additions under that name.
  assert!(!has_upcoming_additions(base, head, "Upcoming"));
}
