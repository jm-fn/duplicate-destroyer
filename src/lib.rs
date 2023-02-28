//! Duplicate Destroyer Library
//!
//! This library provides functionality to find duplicate files and folders.
//!
//! To search for duplicates in a set of directories, call `get_duplicates` function. The function
//! goes recursively through all the directories in its input and finds all duplicate files and
//! directories. It then returns the topmost directories and files for which there exists at least
//! one duplicate.
//!
//! ### Options
//! The `get_duplicates` function has a `options` argument which is (for now) a
//! [HashMap](std::collections::HashMap) of the
//! option keywords and their numerical values. The accepted keywords are:
//! * _"num_threads"_ - The number of threads spawned for calculating the checksums of files
//! [default = 0]
//! * _"min_size"_ - The minimum size of duplicate objects returned. [default = 0]<br/>
//!    This option has no bearing on the length of calculation, only on the output size. We
//!    calculate checksums of objects smaller than _min_size_ (if they might have duplicates),
//!    since otherwise we would not get the duplicates of directories containing objects smaller
//!    than _min_size_.
//!
//! All the keywords in the list above are optional and all other keywords are silently ignored.
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
//! use std::collections::{HashSet, HashMap};
//! use duplicate_destroyer::*;
//! use std::ffi::OsString;
//!
//!
//! let options = HashMap::from([(String::from("num_threads"), 0_u64),
//!                             (String::from("min_size"), 10_u64)]);
//! let input_dirs = vec![OsString::from("tests/fixtures")];
//! let duplicates = duplicate_destroyer::get_duplicates(input_dirs, options).unwrap();
//!
//! let expected_output = DuplicateObject::new(8235,
//!                                            HashSet::from([OsString::from("tests/fixtures/A"),
//!                                            OsString::from("tests/fixtures/B/A")]));
//! assert_eq!(duplicates[0], expected_output)
//! ```

mod checksum;
mod dir_tree;
mod duplicate_object;
mod duplicate_table;

pub use duplicate_object::DuplicateObject;

use std::collections::HashMap;
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
/// * `options` - HashMap of settings, see [options](crate#options)
pub fn get_duplicates(
    directories: Vec<OsString>,
    mut options: HashMap<String, u64>,
) -> Result<Vec<DuplicateObject>, DuDeError> {
    let num_threads: usize = options.remove("num_threads").unwrap_or(0) as usize;
    let mut tree = dir_tree::DirTree::new(num_threads);
    tree.add_directories(directories);

    log::debug!("Finished adding directories");
    let min_size = options.remove("min_size").unwrap_or(0);
    tree.finalise();
    let mut duplicates = tree.get_duplicates(min_size);
    duplicates.sort_by_key(|x| x.size);
    duplicates.reverse();

    Ok(duplicates)
}
