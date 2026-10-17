use std::collections::HashMap;
use std::fmt;

pub const MAX_ITEMS: usize = 64;
pub const MAX_LABELS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelError {
    TooManyItems,
    InvalidLabel,
    TooManyLabels,
}
impl fmt::Display for LabelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooManyItems => "at most 64 input items are supported",
            Self::InvalidLabel => "labels must contain 1 to 16 lowercase ASCII letters",
            Self::TooManyLabels => "at most 8 distinct labels are supported",
        })
    }
}
impl std::error::Error for LabelError {}

/// The borrowed slice can come from an array or a Vec. Empty input has no extrema.
pub fn sample_bounds(samples: &[i32]) -> Option<(i32, i32)> {
    let (&first, rest) = samples.split_first()?;
    let mut low = first;
    let mut high = first;
    for &sample in rest {
        low = low.min(sample);
        high = high.max(sample);
    }
    Some((low, high))
}

/// Count a bounded batch and return owned rows in lexical label order.
/// Input borrows stay local; rejected input exposes no partial result.
pub fn count_labels(labels: &[&str]) -> Result<Vec<(String, u32)>, LabelError> {
    if labels.len() > MAX_ITEMS {
        return Err(LabelError::TooManyItems);
    }
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for &label in labels {
        if label.is_empty() || label.len() > 16 || !label.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(LabelError::InvalidLabel);
        }
        if !counts.contains_key(label) && counts.len() == MAX_LABELS {
            return Err(LabelError::TooManyLabels);
        }
        let count = counts.entry(label).or_insert(0);
        // Every increment corresponds to one input item, and MAX_ITEMS is 64.
        *count += 1;
    }
    let mut rows = Vec::with_capacity(counts.len());
    for (label, count) in counts {
        rows.push((label.to_owned(), count));
    }
    rows.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrays_and_vectors_share_a_slice_contract() {
        let fixed = [12, -8, 15, 9];
        let variable = vec![12, -8, 15, 9];
        assert_eq!(sample_bounds(&fixed), Some((-8, 15)));
        assert_eq!(sample_bounds(&variable), sample_bounds(&fixed));
        assert_eq!(sample_bounds(&[]), None);
        assert_eq!(sample_bounds(&[i32::MIN]), Some((i32::MIN, i32::MIN)));
    }

    #[test]
    fn repeated_labels_are_counted_and_rows_are_deterministic() {
        let expected = vec![("api".to_owned(), 2), ("worker".to_owned(), 1)];
        assert_eq!(
            count_labels(&["worker", "api", "api"]),
            Ok(expected.clone())
        );
        assert_eq!(count_labels(&["api", "worker", "api"]), Ok(expected));
        assert_eq!(count_labels(&[]), Ok(vec![]));
    }

    #[test]
    fn item_distinct_and_text_boundaries_are_independent() {
        assert_eq!(
            count_labels(&["api"; MAX_ITEMS]),
            Ok(vec![("api".into(), 64)])
        );
        assert_eq!(
            count_labels(&["api"; MAX_ITEMS + 1]),
            Err(LabelError::TooManyItems)
        );
        let eight = ["a", "b", "c", "d", "e", "f", "g", "h"];
        assert_eq!(count_labels(&eight).unwrap().len(), MAX_LABELS);
        assert_eq!(
            count_labels(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
            Err(LabelError::TooManyLabels)
        );
        for bad in ["", "API", "a b", "é", "abcdefghijklmnopq"] {
            assert_eq!(count_labels(&["valid", bad]), Err(LabelError::InvalidLabel));
        }
        assert!(count_labels(&["abcdefghijklmnop"]).is_ok());
    }

    #[test]
    fn result_owns_its_labels_after_input_is_gone() {
        let rows = {
            let label = String::from("api");
            count_labels(&[label.as_str()]).unwrap()
        };
        assert_eq!(rows, [("api".into(), 1)]);
    }
}
