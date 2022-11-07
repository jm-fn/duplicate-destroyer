use std::collections::HashSet;
use std::ffi::OsString;

/// Placeholder for duplicate_object. Meant to hold duplicate sets and data about them
pub struct DuplicateObject {
    duplicates: HashSet<OsString>,
}

impl DuplicateObject {
    pub fn new() -> Self {
        DuplicateObject {
            duplicates: HashSet::new(),
        }
    }
}

/// Placeholder for Duplicate Destroyer Error.
/// Maybe unnecessary?
pub struct DuDeError {
    error: String,
}
