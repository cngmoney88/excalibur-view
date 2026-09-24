//! The company's website, excaliburct.com: where people download the program,
//! buy Office and read what it connects to.
//!
//! These are only ever shown as links for a person to click. The program never
//! fetches anything from the website; updates come from the signed release
//! feed, and a license is a file checked offline.

/// The front page.
pub const HOME: &str = "https://excaliburct.com/";
/// How to buy Office, add users or renew updates.
pub const BUY: &str = "https://excaliburct.com/pricing/#buy";
/// What each edition includes.
pub const PRICING: &str = "https://excaliburct.com/pricing/";
/// Where the Office server is downloaded.
pub const OFFICE_SERVER: &str = "https://excaliburct.com/download/#office";
/// Every connection the program makes, and what goes over it.
pub const CONNECTIONS: &str = "https://excaliburct.com/your-data/";
/// Adding users to a licence, or renewing its updates.
pub const ADD: &str = "https://excaliburct.com/add/";

/// Where "Renew or add users" goes for an office that holds a licence.
///
/// The page is told which licence by the part after `#`, which a browser never
/// sends anywhere: it stays on the machine that opened it. And what is sent is
/// not the licence id, which is what fetches the licence file, but SHA-256 of
/// `buy:` and the id, so an address bar or a browser history holds nothing that
/// fetches anything. The licensing service keeps the same value beside each
/// licence, so the purchase lands on this licence and no other: nobody types a
/// company name, and nothing is matched by spelling.
pub fn add_url(license_id: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    match license_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            let digest = Sha256::digest(format!("buy:{id}").as_bytes());
            let reference: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            format!("{ADD}#{reference}")
        }
        None => BUY.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_is_on_the_company_domain() {
        // A link to a domain the company does not hold is a link somebody
        // else can buy. Every one goes to excaliburct.com over HTTPS.
        for url in [HOME, BUY, PRICING, OFFICE_SERVER, CONNECTIONS, ADD, crate::license::BUY_URL] {
            assert!(url.starts_with("https://excaliburct.com/"), "{url}");
        }
    }

    #[test]
    fn an_add_users_link_names_the_licence_without_carrying_its_id() {
        // The same value the licensing service stores (refForId) and
        // Python's sha256(b"buy:evl_abc123").
        let url = add_url(Some(" evl_abc123 "));
        assert_eq!(
            url,
            "https://excaliburct.com/add/#3071ace3bfd430d2623db832fb7166459058b613d747fdb42243cdbb4be45c49"
        );
        assert!(!url.contains("evl_"));
        assert_eq!(add_url(None), BUY);
        assert_eq!(add_url(Some("  ")), BUY);
    }
}

/// Whether a link out to the company's website should be offered at all.
///
/// A sealed seat shows none. Not because a link is dangerous, but because
/// every one of them is an invitation to open a browser on a machine whose
/// whole point is that it does not talk to the outside world — and because a
/// compliance officer reading down the screen should not find one.
pub fn may_offer_links() -> bool {
    !crate::sealed::is_sealed()
}

#[cfg(test)]
mod sealed_tests {
    #[test]
    fn a_sealed_seat_is_offered_no_way_out() {
        crate::sealed::set_for_testing(crate::sealed::Sealed::ByPolicy);
        assert!(!super::may_offer_links());
        crate::sealed::set_for_testing(crate::sealed::Sealed::No);
        assert!(super::may_offer_links());
    }
}
