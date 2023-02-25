use serde::ser::{SerializeSeq, Serializer};
use serde::Serialize;
use std::collections::HashSet;
use std::ffi::OsString;

/// Holds data of duplicate groups that are returned by DuDe.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateObject {
    /// Set of all duplicate paths in group
    #[serde(serialize_with = "osstring_serialize")]
    pub duplicates: HashSet<OsString>,
    /// Size of one element in duplicates
    #[serde(rename = "elementSize")]
    pub size: u64,
}

fn osstring_serialize<S>(hs: &HashSet<OsString>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = s.serialize_seq(Some(hs.len()))?;
    for item in hs.iter() {
        let stringy: String = item
            .to_owned()
            .into_string()
            .unwrap_or_else(|osstr| format!("Error decoding this: {:?}", osstr));
        seq.serialize_element(&stringy)?;
    }
    seq.end()
}

impl DuplicateObject {
    /// Get new DuplicateObject
    pub fn new(size: u64, duplicates: HashSet<OsString>) -> Self {
        DuplicateObject { duplicates, size }
    }
}

// FIXME: This has to be implemented this way due to a bug, where dirs with empty files are marked
// as duplicate even though they contain different numbers of files. Consider these:
// A ┬ dir1-a
//   └ dir2┬b
//         └c
// B - dir3-d
// When a,b,c,d are all empty files, DuDe will flag dirs A and B as duplicate, even though they
// have different number of descendants. If we create duplicate object from A we will get different
// size than duplicate object created from B, even though they are equivalent.
impl PartialEq for DuplicateObject {
    fn eq(&self, other: &Self) -> bool {
        self.duplicates == other.duplicates
    }
}

impl Eq for DuplicateObject {}

/// Placeholder for Duplicate Destroyer Error.
/// Maybe unnecessary?
#[derive(Debug, Eq, PartialEq)]
pub struct DuDeError {
    error: String,
}
