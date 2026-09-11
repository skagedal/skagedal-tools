//! Deciding whether a ticket covers a given train.
//!
//! Validity on Swedish rail is decided by the train's product name, not by
//! its operator or its route: a Movingo season ticket for one route covers
//! Mälartåg and SJ Regional but not SJ Snabbtåg, SJ InterCity or the night
//! trains. So the tool takes a list of product names from configuration and
//! matches announcements against it. Nothing about any particular ticket is
//! baked in.

use serde::Serialize;

/// The products a ticket covers. `None` means no filtering at all — every
/// departure is reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ticket {
    pub products: Option<Vec<String>>,
}

/// Whether a ticket covers a train.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Coverage {
    /// A product on the ticket's list.
    Covered,
    /// The train has products, and none of them is on the list.
    NotCovered,
    /// The announcement carries no product at all, so nothing can be said.
    /// Treated as not boardable: a departure the ticket may not cover is
    /// worse than no answer.
    Unknown,
}

impl Coverage {
    pub fn is_covered(self) -> bool {
        self == Coverage::Covered
    }
}

impl Ticket {
    /// A ticket that covers everything.
    pub fn unrestricted() -> Self {
        Ticket { products: None }
    }

    /// A ticket restricted to the given product names.
    pub fn for_products<I, S>(products: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ticket {
            products: Some(products.into_iter().map(Into::into).collect()),
        }
    }

    pub fn is_unrestricted(&self) -> bool {
        self.products.is_none()
    }

    /// Classify a train from the product names on its announcement.
    pub fn coverage(&self, products: &[String]) -> Coverage {
        let Some(allowed) = self.products.as_ref() else {
            return Coverage::Covered;
        };
        if products.is_empty() {
            return Coverage::Unknown;
        }
        if products
            .iter()
            .any(|p| allowed.iter().any(|a| matches(p, a)))
        {
            Coverage::Covered
        } else {
            Coverage::NotCovered
        }
    }
}

/// A configured name matches a product when it is the whole name or a prefix
/// of it, ignoring case and surrounding space. The prefix rule is what makes
/// "SJ Regional" match the "SJ Regionaltåg" spelling without also matching
/// "SJ InterCity".
fn matches(product: &str, configured: &str) -> bool {
    let product = normalize(product);
    let configured = normalize(configured);
    !configured.is_empty() && product.starts_with(&configured)
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movingo() -> Ticket {
        Ticket::for_products(["Mälartåg", "SJ Regional"])
    }

    fn products(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn covers_the_listed_products() {
        assert_eq!(
            movingo().coverage(&products(&["Mälartåg"])),
            Coverage::Covered
        );
        assert_eq!(
            movingo().coverage(&products(&["SJ Regional"])),
            Coverage::Covered
        );
    }

    #[test]
    fn rejects_the_products_movingo_does_not_include() {
        for name in ["SJ Snabbtåg", "SJ InterCity", "SJ Nattåg", "Snälltåget"] {
            assert_eq!(
                movingo().coverage(&products(&[name])),
                Coverage::NotCovered,
                "{name} should not be covered"
            );
        }
    }

    #[test]
    fn intercity_is_configuration_not_a_constant() {
        let alla_strackor = Ticket::for_products(["Mälartåg", "SJ Regional", "SJ InterCity"]);
        assert_eq!(
            alla_strackor.coverage(&products(&["SJ InterCity"])),
            Coverage::Covered
        );
    }

    #[test]
    fn matching_ignores_case_and_spacing_and_allows_a_suffix() {
        assert_eq!(
            movingo().coverage(&products(&["sj  regionaltåg"])),
            Coverage::Covered
        );
        assert_eq!(
            movingo().coverage(&products(&[" MÄLARTÅG "])),
            Coverage::Covered
        );
    }

    #[test]
    fn a_train_with_several_products_needs_only_one_match() {
        assert_eq!(
            movingo().coverage(&products(&["SJ Snabbtåg", "Mälartåg"])),
            Coverage::Covered
        );
    }

    #[test]
    fn no_product_information_is_unknown_rather_than_covered() {
        assert_eq!(movingo().coverage(&[]), Coverage::Unknown);
    }

    #[test]
    fn an_unrestricted_ticket_covers_everything_including_the_unknown() {
        let t = Ticket::unrestricted();
        assert_eq!(t.coverage(&products(&["SJ Snabbtåg"])), Coverage::Covered);
        assert_eq!(t.coverage(&[]), Coverage::Covered);
        assert!(t.is_unrestricted());
    }

    #[test]
    fn an_empty_configured_name_matches_nothing() {
        let t = Ticket::for_products([""]);
        assert_eq!(t.coverage(&products(&["Mälartåg"])), Coverage::NotCovered);
    }
}
