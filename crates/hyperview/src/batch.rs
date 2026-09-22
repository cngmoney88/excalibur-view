//! Doing one job to a lot of drawings.
//!
//! A shop receives a submittal as forty separate PDFs and wants one set. It
//! receives a hundred and fifty sheets and wants a job number in the corner of
//! every one. It has last month's issue and this month's and wants to know
//! which sheets moved. Those are the same three operations as the Document
//! menu, run over a list instead of over one file.
//!
//! Two things make a batch different from a loop, and both are about what
//! happens when it goes wrong halfway.
//!
//! **One failure does not stop the rest.** A hundred and fifty sheets where the
//! fourteenth is password-protected should produce a hundred and forty-nine
//! results and one line saying which one could not be done — not fourteen
//! results and a dialog.
//!
//! **The report says what happened to what.** Every file is named, with what
//! was done to it or why it was not. A batch that says "done" and leaves
//! somebody to work out which of forty files it touched is a batch nobody
//! trusts twice.

use std::path::{Path, PathBuf};

use crate::docops::Operation;

/// What to do to every file in a list.
///
/// Held as a template rather than as a list of finished operations, because
/// each one's source and destination depend on the file it lands on, and
/// building them all up front means building a hundred and fifty paths before
/// finding out the first one is unreadable.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// Every file into one.
    CombineAll { to: PathBuf },
    /// Each file split into runs.
    Split { every: usize, into: PathBuf },
    /// Each file stamped.
    Stamp {
        stamps: Vec<crate::stamps::Stamp>,
        into: Option<PathBuf>,
    },
    /// Each file turned.
    Rotate {
        quarter_turns: i32,
        into: Option<PathBuf>,
    },
    /// Each file flattened.
    Flatten { into: Option<PathBuf> },
    /// Each file rendered smaller.
    Shrink { dpi: u32, into: Option<PathBuf> },
    /// Each file read, so a folder of scans becomes searchable in one go.
    Ocr {
        dpi: u32,
        only_scans: bool,
        into: Option<PathBuf>,
    },
    /// Each file sealed.
    Seal {
        picture: PathBuf,
        width: f32,
        into: Option<PathBuf>,
    },
    /// Each file slip-sheeted against the revision of the same name.
    ///
    /// Matched by file name rather than by asking for pairs one at a time,
    /// because the way a revision actually arrives is a folder of files named
    /// after the sets they replace.
    SlipSheet {
        revisions: PathBuf,
        keep_superseded: bool,
        into: Option<PathBuf>,
    },
    /// Every sheet of every set compared against the issue of the same name in
    /// another folder, and the differences clouded on the newer one.
    ///
    /// The morning job, done across a whole set instead of a sheet at a time:
    /// point at last month's issue and this month's, and get back one file per
    /// set with a cloud round everything that moved.
    Compare {
        older: PathBuf,
        darkness: f32,
        into: Option<PathBuf>,
    },
    /// Every sheet of every set trimmed to the same box.
    Crop {
        /// Left, bottom, right, top, in points.
        box_: [f32; 4],
        into: Option<PathBuf>,
    },
    /// The same picture stamped on every sheet of every set.
    ApplyStamp {
        picture: PathBuf,
        width: f32,
        spot: crate::stamps::Spot,
        into: Option<PathBuf>,
    },
    /// Every sheet laid over the sheet of the same name in another folder.
    Overlay {
        older: PathBuf,
        darkness: f32,
        into: Option<PathBuf>,
    },
    /// Every set sent to the plotter.
    Print,
    /// One takeoff report covering the whole list.
    Summary {
        to: PathBuf,
    },
    /// Every set locked with the same password and permissions.
    Secure {
        open_password: String,
        owner_password: String,
        allowed: pdf::crypt::Allowed,
        into: Option<PathBuf>,
    },
}

impl Job {
    pub fn name(&self) -> &'static str {
        match self {
            Job::CombineAll { .. } => "Combine",
            Job::Split { .. } => "Split",
            Job::Stamp { .. } => "Headers and Footers",
            Job::Rotate { .. } => "Rotate",
            Job::Flatten { .. } => "Flatten Markups",
            Job::Shrink { .. } => "Reduce File Size",
            Job::Ocr { .. } => "OCR",
            Job::Seal { .. } => "Sign and Seal",
            Job::SlipSheet { .. } => "Slip Sheet",
            Job::Compare { .. } => "Compare Documents",
            Job::Crop { .. } => "Crop and Page Setup",
            Job::ApplyStamp { .. } => "Apply Stamp",
            Job::Overlay { .. } => "Overlay Pages",
            Job::Print => "Print",
            Job::Summary { .. } => "Summary",
            Job::Secure { .. } => "Security",
        }
    }

    /// Whether this one takes the whole list at once rather than each file in
    /// turn. Combining is the only one, and it is why the runner has to ask.
    pub fn all_at_once(&self) -> bool {
        matches!(self, Job::CombineAll { .. } | Job::Summary { .. })
    }

    /// Where a result goes for one input.
    ///
    /// Beside the file it came from unless somebody named a folder, because
    /// that is where they will look — and never over the top of the original,
    /// which is what [`crate::docops::free_name`] guarantees.
    fn output_for(&self, source: &Path, suffix: &str) -> PathBuf {
        let into = match self {
            Job::Stamp { into, .. }
            | Job::Rotate { into, .. }
            | Job::Flatten { into }
            | Job::Shrink { into, .. }
            | Job::Ocr { into, .. }
            | Job::Seal { into, .. }
            | Job::SlipSheet { into, .. }
            | Job::Compare { into, .. }
            | Job::Crop { into, .. }
            | Job::ApplyStamp { into, .. }
            | Job::Overlay { into, .. }
            | Job::Secure { into, .. } => into.clone(),
            _ => None,
        };
        match into {
            Some(folder) => {
                let name = source
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "drawing".into());
                crate::docops::free_name(&folder.join(format!("{name} {suffix}.pdf")))
            }
            None => crate::docops::beside(source, suffix),
        }
    }

    /// The operation for one file, or `None` when this job does not work that
    /// way.
    pub fn for_one(&self, source: &Path, pages: usize) -> Option<Operation> {
        Some(match self {
            Job::CombineAll { .. } => return None,
            Job::Split { every, into } => Operation::Split {
                source: source.to_path_buf(),
                into: into.clone(),
                every: *every,
            },
            Job::Stamp { stamps, .. } => Operation::Stamp {
                source: source.to_path_buf(),
                pages: (0..pages as u32).collect(),
                stamps: stamps.clone(),
                to: self.output_for(source, "stamped"),
            },
            Job::Rotate { quarter_turns, .. } => Operation::Rotate {
                source: source.to_path_buf(),
                pages: (0..pages as u32).collect(),
                quarter_turns: *quarter_turns,
                to: self.output_for(source, "turned"),
            },
            Job::Flatten { .. } => Operation::Flatten {
                source: source.to_path_buf(),
                pages: (0..pages as u32).collect(),
                to: self.output_for(source, "flattened"),
            },
            Job::Shrink { dpi, .. } => Operation::Shrink {
                source: source.to_path_buf(),
                to: self.output_for(source, "small"),
                dpi: *dpi,
            },
            Job::Ocr { dpi, only_scans, .. } => Operation::Ocr {
                source: source.to_path_buf(),
                pages: (0..pages as u32).collect(),
                to: self.output_for(source, "searchable"),
                dpi: *dpi,
                only_scans: *only_scans,
            },
            Job::Seal { picture, width, .. } => Operation::Seal {
                source: source.to_path_buf(),
                picture: picture.clone(),
                pages: (0..pages as u32).collect(),
                placement: crate::seal::Placement {
                    x: 0.0,
                    y: 0.0,
                    width: *width,
                    corner: None,
                },
                to: self.output_for(source, "sealed"),
            },
            Job::SlipSheet {
                revisions,
                keep_superseded,
                ..
            } => {
                // The revision named after this set. Nothing else will do —
                // guessing which revision goes with which set is how a bid ends
                // up with somebody else's sheets in it.
                let against = matching_revision(source, revisions)?;
                Operation::SlipSheet {
                    source: source.to_path_buf(),
                    revisions: against,
                    to: self.output_for(source, "slipped"),
                    keep_superseded: *keep_superseded,
                }
            }
            Job::Compare { older, darkness, .. } => {
                let against = matching_revision(source, older)?;
                Operation::Compare {
                    newer: source.to_path_buf(),
                    older: against,
                    to: self.output_for(source, "changes clouded"),
                    darkness: *darkness,
                }
            }
            Job::Crop { box_, .. } => Operation::Crop {
                source: source.to_path_buf(),
                pages: (0..pages as u32).collect(),
                box_: *box_,
                to: self.output_for(source, "cropped"),
            },
            Job::ApplyStamp { picture, width, spot, .. } => Operation::Seal {
                source: source.to_path_buf(),
                picture: picture.clone(),
                pages: (0..pages as u32).collect(),
                placement: crate::seal::Placement::at(*spot, *width),
                to: self.output_for(source, "stamped"),
            },
            Job::Overlay { older, darkness, .. } => {
                let against = matching_revision(source, older)?;
                Operation::Overlay {
                    newer: source.to_path_buf(),
                    older: against,
                    to: self.output_for(source, "overlaid"),
                    darkness: *darkness,
                }
            }
            Job::Print => Operation::Print {
                source: source.to_path_buf(),
            },
            Job::Summary { .. } => return None,
            Job::Secure {
                open_password,
                owner_password,
                allowed,
                ..
            } => Operation::Secure {
                source: source.to_path_buf(),
                to: self.output_for(source, "locked"),
                open_password: open_password.clone(),
                owner_password: owner_password.clone(),
                allowed: *allowed,
            },
        })
    }

    /// The single operation for a job that takes the whole list.
    pub fn for_all(&self, sources: &[PathBuf]) -> Option<Operation> {
        match self {
            Job::CombineAll { to } => Some(Operation::Combine {
                sources: sources.to_vec(),
                to: to.clone(),
            }),
            Job::Summary { to } => Some(Operation::Summary {
                sources: sources.to_vec(),
                to: to.clone(),
            }),
            _ => None,
        }
    }
}

/// What happened to one file.
#[derive(Clone, Debug)]
pub struct Line {
    pub source: PathBuf,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub enum Outcome {
    Done { wrote: Vec<PathBuf>, said: String },
    /// Named, with the reason, so somebody can fix that one and run it again.
    Failed(String),
}

impl Outcome {
    pub fn went_well(&self) -> bool {
        matches!(self, Outcome::Done { .. })
    }
}

/// The whole run.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub lines: Vec<Line>,
}

impl Report {
    pub fn done(&self) -> usize {
        self.lines.iter().filter(|l| l.outcome.went_well()).count()
    }

    pub fn failed(&self) -> usize {
        self.lines.len() - self.done()
    }

    pub fn wrote(&self) -> Vec<&PathBuf> {
        self.lines
            .iter()
            .filter_map(|line| match &line.outcome {
                Outcome::Done { wrote, .. } => Some(wrote.iter()),
                Outcome::Failed(_) => None,
            })
            .flatten()
            .collect()
    }

    /// One line for the status bar. The detail is in the report itself.
    pub fn says(&self) -> String {
        let done = self.done();
        let failed = self.failed();
        if failed == 0 {
            return format!(
                "{done} file{} done.",
                if done == 1 { "" } else { "s" }
            );
        }
        format!(
            "{done} done, {failed} could not be — see the list for which and why. \
             Nothing was changed in the files that failed.",
            )
    }

    /// The report as text, for pasting into an email.
    pub fn as_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            let name = line
                .source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| line.source.display().to_string());
            match &line.outcome {
                Outcome::Done { said, .. } => out.push_str(&format!("{name}\n    {said}\n")),
                Outcome::Failed(why) => {
                    out.push_str(&format!("{name}\n    NOT DONE — {why}\n"))
                }
            }
        }
        out
    }
}

/// The revision belonging to one set: the file of the same name in the folder
/// of revisions.
///
/// Exact, after ignoring case and the extension. A near match is not used,
/// because "Structural Rev 2.pdf" and "Structural.pdf" being treated as a pair
/// is the sort of guess that puts the wrong sheets in somebody's bid.
pub fn matching_revision(source: &Path, revisions: &Path) -> Option<PathBuf> {
    let wanted = source.file_stem()?.to_string_lossy().to_lowercase();
    pdfs_in(revisions).into_iter().find(|candidate| {
        candidate
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase() == wanted)
            .unwrap_or(false)
    })
}

/// Every PDF in a folder, in the order a person would read them.
///
/// Sorted by name, and by the numbers inside the names rather than by
/// character, so "Sheet 2" comes before "Sheet 10" the way somebody expects
/// rather than after it the way a computer would.
pub fn pdfs_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
        })
        .collect();
    found.sort_by(|a, b| naturally(&name_of(a), &name_of(b)));
    found
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Compares two names the way a person reads them: runs of digits as numbers.
pub fn naturally(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(cx), Some(cy)) => {
                if cx.is_ascii_digit() && cy.is_ascii_digit() {
                    let nx: String = take_digits(&mut x);
                    let ny: String = take_digits(&mut y);
                    // Leading zeros do not make a number bigger, and a number
                    // too long to fit in a u64 is compared by length then text.
                    let tx = nx.trim_start_matches('0');
                    let ty = ny.trim_start_matches('0');
                    let by_length = tx.len().cmp(&ty.len());
                    if by_length != std::cmp::Ordering::Equal {
                        return by_length;
                    }
                    let by_digits = tx.cmp(ty);
                    if by_digits != std::cmp::Ordering::Equal {
                        return by_digits;
                    }
                } else {
                    let ordering = cx
                        .to_ascii_lowercase()
                        .cmp(&cy.to_ascii_lowercase());
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                    x.next();
                    y.next();
                }
            }
        }
    }
}

fn take_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(c) = chars.peek().copied() {
        if !c.is_ascii_digit() {
            break;
        }
        out.push(c);
        chars.next();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_a_failure_in_it_says_so_rather_than_saying_done() {
        let report = Report {
            lines: vec![
                Line {
                    source: PathBuf::from("One.pdf"),
                    outcome: Outcome::Done {
                        wrote: vec![PathBuf::from("One stamped.pdf")],
                        said: "1 sheet stamped".into(),
                    },
                },
                Line {
                    source: PathBuf::from("Two.pdf"),
                    outcome: Outcome::Failed("Two.pdf is password protected.".into()),
                },
            ],
        };
        assert_eq!(report.done(), 1);
        assert_eq!(report.failed(), 1);
        let said = report.says();
        assert!(said.contains("1 done, 1 could not"), "{said}");
        // And it says the files that failed were not half-changed.
        assert!(said.contains("Nothing was changed"), "{said}");
    }

    #[test]
    fn the_report_names_every_file_and_why() {
        let report = Report {
            lines: vec![Line {
                source: PathBuf::from("/jobs/Structural.pdf"),
                outcome: Outcome::Failed("it is password protected".into()),
            }],
        };
        let text = report.as_text();
        assert!(text.contains("Structural.pdf"));
        assert!(text.contains("NOT DONE"));
        assert!(text.contains("password"));
    }

    #[test]
    fn a_batch_writes_beside_each_file_unless_told_a_folder() {
        let job = Job::Rotate {
            quarter_turns: 1,
            into: None,
        };
        let to = job.output_for(Path::new("/jobs/24-118/Structural.pdf"), "turned");
        assert!(to.starts_with("/jobs/24-118"));
        assert!(to.to_string_lossy().contains("turned"));

        let job = Job::Rotate {
            quarter_turns: 1,
            into: Some(PathBuf::from("/out")),
        };
        let to = job.output_for(Path::new("/jobs/24-118/Structural.pdf"), "turned");
        assert!(to.starts_with("/out"), "{}", to.display());
    }

    #[test]
    fn a_batch_never_writes_over_the_file_it_is_reading() {
        for job in [
            Job::Rotate { quarter_turns: 1, into: None },
            Job::Flatten { into: None },
            Job::Shrink { dpi: 150, into: None },
            Job::Stamp { stamps: Vec::new(), into: None },
        ] {
            let source = Path::new("/jobs/Structural.pdf");
            if let Some(op) = job.for_one(source, 4) {
                let out = op.output().expect("somewhere to write");
                assert_ne!(out, source, "{:?}", job.name());
            }
        }
    }

    #[test]
    fn combining_is_the_one_that_takes_the_whole_list() {
        let job = Job::CombineAll { to: PathBuf::from("All.pdf") };
        assert!(job.all_at_once());
        assert!(job.for_one(Path::new("a.pdf"), 1).is_none());
        let all = job
            .for_all(&[PathBuf::from("a.pdf"), PathBuf::from("b.pdf")])
            .expect("a combine");
        match all {
            Operation::Combine { sources, .. } => assert_eq!(sources.len(), 2),
            _ => panic!("that is a combine"),
        }

        // And the others are the other way round.
        let each = Job::Flatten { into: None };
        assert!(!each.all_at_once());
        assert!(each.for_all(&[PathBuf::from("a.pdf")]).is_none());
        assert!(each.for_one(Path::new("a.pdf"), 1).is_some());
    }

    #[test]
    fn files_sort_the_way_a_person_reads_them() {
        use std::cmp::Ordering;
        // The one that matters: 2 before 10.
        assert_eq!(naturally("Sheet 2.pdf", "Sheet 10.pdf"), Ordering::Less);
        assert_eq!(naturally("Sheet 10.pdf", "Sheet 2.pdf"), Ordering::Greater);
        assert_eq!(naturally("S-100", "S-99"), Ordering::Greater);
        assert_eq!(naturally("A.pdf", "a.pdf"), Ordering::Equal);
        assert_eq!(naturally("", ""), Ordering::Equal);
        // Leading zeros do not change a number's size.
        assert_eq!(naturally("Sheet 007", "Sheet 7"), Ordering::Equal);
        assert_eq!(naturally("Sheet 007", "Sheet 8"), Ordering::Less);
    }

    #[test]
    fn a_folder_with_nothing_in_it_is_an_empty_list_rather_than_a_failure() {
        assert!(pdfs_in(Path::new("/no/such/folder/at/all")).is_empty());
    }
}

#[cfg(test)]
mod pairing_tests {
    use super::*;

    #[test]
    fn a_revision_is_paired_with_the_set_of_the_same_name() {
        let dir = std::env::temp_dir().join(format!(
            "hyperview-pairing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        let revisions = dir.join("Rev 2");
        std::fs::create_dir_all(&revisions).unwrap();
        std::fs::write(revisions.join("Structural.pdf"), b"%PDF-1.7\n").unwrap();
        std::fs::write(revisions.join("Architectural.pdf"), b"%PDF-1.7\n").unwrap();

        let found = matching_revision(Path::new("/jobs/Structural.pdf"), &revisions);
        assert_eq!(found.as_deref().and_then(|p| p.file_name()), Some("Structural.pdf".as_ref()));

        // Case does not matter, because two people named these files.
        let found = matching_revision(Path::new("/jobs/STRUCTURAL.pdf"), &revisions);
        assert!(found.is_some());

        // And a near miss is not a match.
        assert!(matching_revision(Path::new("/jobs/Structural Rev 2.pdf"), &revisions).is_none());
        assert!(matching_revision(Path::new("/jobs/Mechanical.pdf"), &revisions).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_set_with_no_revision_is_left_out_rather_than_paired_with_something_else() {
        // for_one gives None, which the runner reports as a line saying so —
        // not a file quietly slipped against the wrong revision.
        let job = Job::SlipSheet {
            revisions: PathBuf::from("/no/such/folder"),
            keep_superseded: false,
            into: None,
        };
        assert!(job.for_one(Path::new("/jobs/Structural.pdf"), 12).is_none());
    }
}
