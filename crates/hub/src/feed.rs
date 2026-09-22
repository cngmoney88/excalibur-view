//! The publisher's release feed.
//!
//! One place on the internet says what the newest Hyperview is: a releases
//! page on GitHub, where each release carries the two programs and a
//! `release.json` describing them, each description signed by the key Creede
//! holds. A company's server reads it and passes new versions on to its own
//! seats; a seat with no server reads it for itself.
//!
//! Nothing here decides whether a build is trustworthy. This only fetches.
//! What arrives is checked against the keys compiled into the running program
//! by [`crate::update::Trusted::check`] before anything is done with it, so a
//! feed that was taken over could serve only builds that were already signed
//! by the publisher's own key.
//!
//! Nothing goes the other way. The feed is asked what exists and is sent
//! nothing about who is asking: not a company name, not a drawing, not a count
//! of seats.

use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::update::{compare, Channel, Release};

/// The file on every release that describes it.
pub const MANIFEST: &str = "release.json";
/// What the viewer is published as.
pub const APP_PLATFORM: &str = "windows-x64";
/// What the server is published as. A different platform string, so a
/// signature over one program can never be passed off as a signature over the
/// other: the platform is part of what is signed.
pub const SERVER_PLATFORM: &str = "windows-x64-server";

/// Larger than any build will be, and small enough that a feed serving
/// something enormous cannot fill a server's disk.
const LARGEST: u64 = 400 * 1024 * 1024;

/// What one release says about itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Published {
    /// The viewer. Its `download` is the name of the file on the release.
    pub app: Release,
    /// The server, when this release includes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Release>,
}

/// The newest release a feed has for a channel, and where its files are.
#[derive(Clone, Debug)]
pub struct Found {
    pub published: Published,
    files: Vec<(String, String)>,
}

impl Found {
    /// Where to download one of the release's files, by the name its manifest
    /// gives it.
    pub fn url(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.as_str())
    }
}

#[derive(Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct Feed {
    url: String,
    agent: ureq::Agent,
}

impl Feed {
    /// `url` is the releases list, e.g.
    /// `https://api.github.com/repos/<owner>/<repo>/releases`.
    pub fn new(url: &str) -> Feed {
        Feed {
            url: url.trim().to_string(),
            agent: crate::web::agent()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(300))
                .user_agent(concat!("excalibur-hyperview/", env!("CARGO_PKG_VERSION")))
                .build(),
        }
    }

    /// The newest release on a channel, or `None` when there is nothing.
    ///
    /// A stable follower sees only full releases whose manifest says stable.
    /// A preview follower — the shop trying the next one first — sees
    /// prereleases too.
    pub fn newest(&self, channel: Channel) -> Result<Option<Found>, String> {
        crate::web::outbound("Checking for a new version", &self.url)?;
        let listed: Vec<Listed> = self
            .agent
            .get(&self.url)
            .set("Accept", "application/vnd.github+json")
            .query("per_page", "20")
            .call()
            .map_err(|e| format!("the release feed did not answer: {e}"))?
            .into_json()
            .map_err(|e| format!("the release feed's answer is not readable: {e}"))?;
        choose(listed, channel, |url| self.text(url))
    }

    /// Downloads one file off a release.
    pub fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        crate::web::outbound("Downloading a new version", url)?;
        let response = self
            .agent
            .get(url)
            .set("Accept", "application/octet-stream")
            .call()
            .map_err(|e| format!("could not download {url}: {e}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(LARGEST)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("the download stopped part way: {e}"))?;
        Ok(bytes)
    }

    fn text(&self, url: &str) -> Result<String, String> {
        let bytes = self.fetch(url)?;
        String::from_utf8(bytes).map_err(|_| "a release manifest is not text".to_string())
    }
}

/// The choosing, apart from the fetching, so it can be tested without a
/// network.
fn choose(
    listed: Vec<Listed>,
    channel: Channel,
    read: impl Fn(&str) -> Result<String, String>,
) -> Result<Option<Found>, String> {
    let mut candidates: Vec<Listed> = listed
        .into_iter()
        .filter(|r| !r.draft)
        .filter(|r| channel == Channel::Preview || !r.prerelease)
        .filter(|r| r.assets.iter().any(|a| a.name == MANIFEST))
        .collect();
    // Newest by version, not by the order the page happens to list them in.
    candidates.sort_by(|a, b| compare(bare(&b.tag_name), bare(&a.tag_name)));

    for listed in candidates {
        let Some(manifest) = listed.assets.iter().find(|a| a.name == MANIFEST) else {
            continue;
        };
        let text = read(&manifest.browser_download_url)?;
        let published: Published = match serde_json::from_str(&text) {
            Ok(p) => p,
            // One broken release does not hide an older good one.
            Err(_) => continue,
        };
        if channel == Channel::Stable && published.app.channel != Channel::Stable {
            continue;
        }
        let files = listed
            .assets
            .into_iter()
            .map(|a| (a.name, a.browser_download_url))
            .collect();
        return Ok(Some(Found { published, files }));
    }
    Ok(None)
}

fn bare(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str, channel: Channel) -> Release {
        Release {
            version: version.into(),
            channel,
            platform: APP_PLATFORM.into(),
            published: "2026-09-21T12:00:00Z".into(),
            notes: String::new(),
            download: "Hyperview.exe".into(),
            bytes: 1,
            digest: "00".into(),
            signature: "00".into(),
            key: "mesafab-2026".into(),
            minimum_api_version: 1,
        }
    }

    fn listed(tag: &str, prerelease: bool) -> Listed {
        Listed {
            tag_name: tag.into(),
            draft: false,
            prerelease,
            assets: vec![
                Asset {
                    name: MANIFEST.into(),
                    browser_download_url: format!("https://feed/{tag}/release.json"),
                },
                Asset {
                    name: "Hyperview.exe".into(),
                    browser_download_url: format!("https://feed/{tag}/Hyperview.exe"),
                },
            ],
        }
    }

    fn reader(url: &str) -> Result<String, String> {
        let tag = url.split('/').nth(3).unwrap_or("");
        let version = bare(tag);
        let channel = if version.ends_with('9') {
            Channel::Preview
        } else {
            Channel::Stable
        };
        Ok(serde_json::to_string(&Published {
            app: release(version, channel),
            server: None,
        })
        .unwrap())
    }

    #[test]
    fn the_newest_by_version_wins_not_the_first_listed() {
        let found = choose(
            vec![listed("v0.4.1", false), listed("v0.10.0", false), listed("v0.5.0", false)],
            Channel::Stable,
            reader,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.published.app.version, "0.10.0");
        assert_eq!(found.url("Hyperview.exe"), Some("https://feed/v0.10.0/Hyperview.exe"));
    }

    #[test]
    fn a_stable_follower_never_sees_a_prerelease() {
        let found = choose(
            vec![listed("v0.4.1", false), listed("v0.5.9", true)],
            Channel::Stable,
            reader,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.published.app.version, "0.4.1");
    }

    #[test]
    fn the_shop_trying_things_first_does_see_it() {
        let found = choose(
            vec![listed("v0.4.1", false), listed("v0.5.9", true)],
            Channel::Preview,
            reader,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.published.app.version, "0.5.9");
    }

    #[test]
    fn a_release_whose_manifest_says_preview_is_not_stable_just_because_it_was_not_ticked() {
        // v0.4.9 is a full release on the page, but its signed manifest says
        // preview. The signature decides, not the checkbox.
        let found = choose(
            vec![listed("v0.4.1", false), listed("v0.4.9", false)],
            Channel::Stable,
            reader,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.published.app.version, "0.4.1");
    }

    #[test]
    fn drafts_and_releases_without_a_manifest_are_ignored() {
        let mut draft = listed("v0.9.0", false);
        draft.draft = true;
        let mut bare_release = listed("v0.8.0", false);
        bare_release.assets.retain(|a| a.name != MANIFEST);
        let found = choose(
            vec![draft, bare_release, listed("v0.4.1", false)],
            Channel::Stable,
            reader,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.published.app.version, "0.4.1");
    }

    #[test]
    fn an_empty_feed_is_nothing_new_not_an_error() {
        assert!(choose(Vec::new(), Channel::Stable, reader).unwrap().is_none());
    }
}
