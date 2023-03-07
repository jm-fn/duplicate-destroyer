//! Duplicate Destroyer Library
//!
//! This library provides functionality to find duplicate files and folders.
//!
//! To search for duplicates in a set of directories, call `get_duplicates` function. The function
//! goes recursively through all the directories in its input and finds all duplicate files and
//! directories. It then returns the topmost directories and files for which there exists at least
//! one duplicate.
//!
//! # Example usage
//! Suppose we have directory structure:
//! ```bash
//! tests/fixtures/
//! ├── A
//! │   ├── a.txt
//! │   └── b
//! │       ├── alpha.txt
//! │       └── beta.txt
//! ├── B
//! │   └── A
//! │       ├── a.txt
//! │       └── b
//! │           ├── alpha.txt
//! │           └── beta.txt
//! └── C
//!     ├── a.txt
//!     ├── b
//!     │   ├── alpha.txt
//!     │   └── beta.txt
//!     └── diff.txt
//!
//! ```
//! The `get_duplicates` function would then return these directories as duplicates
//!  {"tests/fixtures/A", "tests/fixtures/B/A"}:
//!
//! ```
//! # use std::collections::HashSet;
//! # use std::ffi::OsString;
//! use duplicate_destroyer::*;
//!
//! // Create DuDe configuration
//! let mut config: Config = Default::default();
//! config.set_minimum_size(0); // Use non-default minimum size (see Config structure for details)
//!
//! // Create vector of paths to search for duplicates
//! let input_dirs = vec![OsString::from("tests/fixtures")];
//!
//! // Get duplicates
//! let duplicates = duplicate_destroyer::get_duplicates(input_dirs, config).unwrap();
//!
//! let expected_paths = [OsString::from("tests/fixtures/A"),
//!                       OsString::from("tests/fixtures/B/A")];
//! let expected_output = DuplicateObject::new(8235, HashSet::from(expected_paths));
//! assert_eq!(duplicates[0], expected_output)
//! ```

mod checksum;
mod config;
mod dir_tree;
mod duplicate_object;
mod duplicate_table;

pub use config::Config;
pub use duplicate_object::DuplicateObject;

use std::ffi::OsString;

use duplicate_object::*;

/// Find the largest duplicate directories or files
///
/// Goes recursively through each dir in `directories` and finds all duplicated files and
/// directories. Ouputs the topmost directory or file that is duplicated within the dirs in
/// `directories`.
///
/// # Arguments:
/// * `directories` - vector of paths that will be searched for duplicates
/// * `config` - configuration of duplicate destroyer. See [`Config`](crate::Config) struct
pub fn get_duplicates(
    directories: Vec<OsString>,
    config: Config,
) -> Result<Vec<DuplicateObject>, DuDeError> {
    let num_threads: usize = config.get_num_threads();
    let mut tree = dir_tree::DirTree::new(num_threads);
    tree.add_directories(directories);

    log::debug!("Finished adding directories");
    let min_size = config.get_minimum_size();
    tree.finalise();
    let mut duplicates = tree.get_duplicates(min_size);
    duplicates.sort_by_key(|x| x.size);
    duplicates.reverse();

    Ok(duplicates)
}
