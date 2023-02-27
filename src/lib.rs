//! Duplicate Destroyer Library
//!
//! This library provides functionality to find duplicate files and folders and to manipulate with
//! the found duplicates.
//!
//! The library is still in construction. In the desired form it will be able to find the largest
//! duplicate folders and will be able to delete them or replace the contents with soft/hard links.

mod checksum;
mod dir_tree;
mod duplicate_object;
mod duplicate_table;

pub use duplicate_object::DuplicateObject;

use std::collections::HashMap;
use std::ffi::OsString;

use duplicate_object::*;

#[cfg(not(test))]
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
