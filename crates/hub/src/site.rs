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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_is_on_the_company_domain() {
        // A link to a domain the company does not hold is a link somebody
        // else can buy. Every one goes to excaliburct.com over HTTPS.
        for url in [HOME, BUY, PRICING, OFFICE_SERVER, CONNECTIONS, crate::license::BUY_URL] {
            assert!(url.starts_with("https://excaliburct.com/"), "{url}");
        }
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
