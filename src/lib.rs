//! Duplicate Destroyer Library
//!
//! This library provides functionality to find duplicate files and folders and to manipulate with
//! the found duplicates.
//!
//! The library is still in construction. In the desired form it will be able to find the largest
//! duplicate folders and will be able to delete them or replace the contents with soft/hard links.

mod checksum;
pub mod dir_tree;
mod duplicate_object;
mod duplicate_table;


use std::ffi::OsString;

use duplicate_object::*;

#[cfg(not(test))]
pub fn get_duplicates(directories: Vec<OsString>) -> Result<Vec<DuplicateObject>, DuDeError> {
    let mut tree = dir_tree::DirTree::new();
    tree.add_directories(directories);
    let duplicates = tree.get_duplicates();
    println!("Duplicates:\n{:?}", duplicates);
    let mut s = String::new();
    tree.print(&mut s);

    println!("\n\n");
    println!("{s}");

    Ok(vec![DuplicateObject::new(1, std::collections::HashSet::new())])
}
