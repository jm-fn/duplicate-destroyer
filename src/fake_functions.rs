//! Module with fake stuff that gets used in unit tests.

use mockall::predicate::*;
use mockall::*;
use std::ffi::OsString;

use crate::dir_tree::MockWithMetadata;

/// Fake read_dir function for DirTree unit tests. Returns an iterator of MockWithMetadata objects.
pub(crate) fn read_dir(
    path: OsString,
) -> Result<std::vec::IntoIter<Option<MockWithMetadata>>, String> {

    if path == "FAIL" {
        return Err(String::from("Test read_dir failed."));
    }

    if path == "FILE" {
        let mut file = MockWithMetadata::new();
        let metadata = Metadata::new(10, false);
        file.expect_metadata().return_once(|| Ok(metadata));
        file.expect_filepath().return_const("Some/test/file.");

        return Ok(vec![Some(file)].into_iter());
    }

    Err(String::from("Testing failure."))
}

/// Fake Metadata structure to unittest DirTree structure
pub struct Metadata {
    is_dir: bool,
    len: u64,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        return self.len.clone();
    }

    pub fn is_dir(&self) -> bool {
        if self.is_dir {
            return true;
        }
        false
    }

    pub fn new(len: u64, is_dir: bool) -> Self {
        Metadata { len, is_dir }
    }
}
