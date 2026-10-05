//! Categories are slash paths: `car` is a category, and so is `car/fuel`,
//! which sits inside it. Ancestry goes by whole segments, so `car` holds
//! `car/fuel` but not `carpets`.

/// Top-level categories that move money rather than spend it.
pub const NOT_SPENDING: [&str; 3] = ["transfer", "income", "refunds"];

/// The first segment: `car` for `car/fuel`.
pub fn top(category: &str) -> &str {
    category.split('/').next().unwrap_or("")
}

/// How many segments deep: 1 for `car`, 2 for `car/fuel`.
pub fn depth(category: &str) -> usize {
    category.split('/').count()
}

/// Whether `category` is `ancestor` or lies somewhere below it.
pub fn is_within(category: &str, ancestor: &str) -> bool {
    !ancestor.is_empty()
        && category
            .strip_prefix(ancestor)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether money in this category counts as spending. Uncategorised does.
pub fn is_spending(category: &str) -> bool {
    !NOT_SPENDING.contains(&top(category))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestry_goes_by_whole_segments() {
        assert!(is_within("car/fuel", "car"));
        assert!(is_within("car", "car"));
        assert!(is_within("car/fuel/diesel", "car/fuel"));
        assert!(!is_within("carpets", "car"));
        assert!(!is_within("car", "car/fuel"));
        assert!(!is_within("", "car"));
        assert!(!is_within("car", ""));
    }

    #[test]
    fn top_and_depth() {
        assert_eq!(top("food/groceries"), "food");
        assert_eq!(top("car"), "car");
        assert_eq!(depth("car"), 1);
        assert_eq!(depth("car/fuel"), 2);
    }

    #[test]
    fn not_spending_matches_the_top_segment() {
        assert!(!is_spending("transfer"));
        assert!(!is_spending("transfer/saving"));
        assert!(!is_spending("income/salary"));
        assert!(is_spending("transfers"));
        assert!(is_spending("food/groceries"));
        assert!(is_spending(""));
    }
}
