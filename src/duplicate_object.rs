use std::collections::HashSet;
use std::ffi::OsString;

/// Holds data of duplicate groups that are returned by DuDe.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DuplicateObject {
    /// Set of all duplicate paths in group
    pub duplicates: HashSet<OsString>,
    /// Size of one element in duplicates
    pub size: u64,
}

impl DuplicateObject {
    /// Get new DuplicateObject
    pub fn new(size: u64, duplicates: HashSet<OsString>) -> Self {
        DuplicateObject { duplicates, size }
    }

    /// Check whether path is contained in DuplicateObject or if DuplicateObject contains path
    ///
    /// Path is contained in DuplicateObject if one of its members  contains the path in its
    /// subtree. The DuplicateObject is contained in path, if one of its members is contained in
    /// one of the path's subdirectories.
    ///
    /// # Arguments
    /// * `path` - path of file that might be contained in one of the duplicates
    ///
    /// # Returns
    /// * ContEnum::IsContained when the path is contained in self
    /// * ContEnum::Contains when self is contained in path
    pub fn contained(&self, path: &OsString) -> ContEnum {
        for do_path in &self.duplicates {
            let do_str_path = do_path
                .to_str()
                .expect("The path {do_path:?} is not a valid UTF-8 string");
            let str_path = path
                .to_str()
                .expect("The path {path:?} is not a valid UTF-8 string");
            if str_path.contains(do_str_path) {
                return ContEnum::IsContained;
            }
            if do_str_path.contains(str_path) {
                return ContEnum::Contains;
            }
        }
        ContEnum::NotRelated
    }
}

/// Enum that signals if a path is contained in/contains DuplicateObject
#[derive(Debug)]
pub enum ContEnum {
    /// Path is contained in Duplicate object
    IsContained,
    NotRelated,
    /// Duplicate object is contained in path
    Contains,
}

impl ContEnum {
    pub(crate) fn is_not_related(&self) -> bool {
        if let ContEnum::NotRelated = self {
            true
        } else {
            false
        }
    }

    pub fn is_contained(&self) -> bool {
        if let ContEnum::IsContained = self {
            true
        } else {
            false
        }
    }
}

/// Placeholder for Duplicate Destroyer Error.
/// Maybe unnecessary?
#[derive(Debug)]
pub struct DuDeError {
    error: String,
}
