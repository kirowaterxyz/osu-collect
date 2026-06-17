use super::home::InputField;
use crate::mirrors::Mirror;

const ROW_LABEL: &str = "Custom mirror URL";
const ROW_PLACEHOLDER: &str = "https://example.com/d/{id}";

/// Editable list of custom mirror URL rows, shared by the home and config tabs.
///
/// Invariant: never empty, and the **last** row is always the empty "add new"
/// entry slot. Typing into that slot grows the list (a fresh empty trailing row
/// is appended via [`ensure_trailing_empty`](CustomMirrorList::ensure_trailing_empty));
/// [`compact`](CustomMirrorList::compact) drops interior empties when focus
/// leaves the section, so a cleared row disappears.
pub struct CustomMirrorList {
    rows: Vec<InputField>,
}

impl CustomMirrorList {
    fn empty_row() -> InputField {
        InputField::new(ROW_LABEL, "", ROW_PLACEHOLDER)
    }

    /// Build from persisted templates, appending the trailing empty entry slot.
    pub fn from_templates(templates: &[&str]) -> Self {
        let mut rows: Vec<InputField> = templates
            .iter()
            .map(|template| InputField::new(ROW_LABEL, *template, ROW_PLACEHOLDER))
            .collect();
        rows.push(Self::empty_row());
        Self { rows }
    }

    pub fn rows(&self) -> &[InputField] {
        &self.rows
    }

    /// Number of rows, including the trailing empty entry slot. Always ≥ 1.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn row(&self, idx: usize) -> Option<&InputField> {
        self.rows.get(idx)
    }

    pub fn row_mut(&mut self, idx: usize) -> Option<&mut InputField> {
        self.rows.get_mut(idx)
    }

    /// Ensure a trailing empty row exists so there is always a slot for the next
    /// URL. Call after every edit to a custom row.
    pub fn ensure_trailing_empty(&mut self) {
        let needs = self
            .rows
            .last()
            .map(|row| !row.value.trim().is_empty())
            .unwrap_or(true);
        if needs {
            self.rows.push(Self::empty_row());
        }
    }

    /// Drop interior empty rows, keeping exactly one trailing empty slot. Call
    /// when focus leaves the custom-mirror section so cleared rows disappear.
    pub fn compact(&mut self) {
        self.rows.retain(|row| !row.value.trim().is_empty());
        self.rows.push(Self::empty_row());
    }

    /// Count of rows holding a valid custom-mirror template (for summary metrics).
    pub fn valid_count(&self) -> usize {
        self.rows.iter().filter(|row| Self::is_valid(row)).count()
    }

    /// All non-empty trimmed templates regardless of validity, for persistence —
    /// keeping a typo'd URL so the user can fix it instead of losing it on save.
    pub fn nonempty_templates(&self) -> Vec<Box<str>> {
        self.rows
            .iter()
            .map(|row| row.value.trim())
            .filter(|template| !template.is_empty())
            .map(Box::from)
            .collect()
    }

    /// Build a [`Mirror`] for every valid template, in row order. `video=false`
    /// strips video where the (custom) mirror supports it (a no-op for customs).
    pub fn build_mirrors(&self, video: bool) -> Vec<Mirror> {
        self.rows
            .iter()
            .filter_map(|row| {
                let template = row.value.trim();
                let mirror = Mirror::custom(template).ok()?;
                Some(if video { mirror } else { mirror.no_video() })
            })
            .collect()
    }

    fn is_valid(row: &InputField) -> bool {
        let template = row.value.trim();
        !template.is_empty() && Mirror::validate_template(template).is_ok()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/custom_mirrors.rs"]
mod tests;
