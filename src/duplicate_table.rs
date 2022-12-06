//! Table storing file checksums
//!
//! This module provides a table that enables discovery of files with the same checksum. To get the
//! duplicates we create a table, register the files and then call get_duplicates for each data we
//! created.
//!
//! The duplicate table is a hash map with keys being the partial checksums of first n
//! bytes of files. If there is only one item with the partial checksum, the value in
//! DuplicateTable is a single entry of TableData.
//!
//! If there are multiple items with the same partial checksum, we create a hash table with the
//! checksums of the whole files as keys and vectors of the corresponding files as values. We then
//! use this hash table as a value corresponding to the partial checksum.
//!
//! To get the duplicates of an item we check the value corresponding to the partial checksum and if there are
//! multiple entries, we get the vector containing the specified item.
use std::collections::{HashMap, HashSet};

use crate::checksum::get_checksum;
use crate::dir_tree::TableData;

#[derive(Debug)]
pub(crate) struct DuplicateTable {
    table: HashMap<String, DTEntry>,
}

impl DuplicateTable {
    /// Create new empty DuplicateTable
    pub(crate) fn new() -> Self {
        DuplicateTable {
            table: HashMap::new(),
        }
    }

    /// Adds a file to duplicate table.
    ///
    /// # Arguments
    /// `part_checksum` - partial checksum of the file
    /// `data` - table data corresponding to the file
    pub(crate) fn register_item(&mut self, part_checksum: String, data: TableData) {
        match self.table.get(&part_checksum) {
            // There is single entry for part_checksum key
            Some(DTEntry::Single(_)) => {
                // change value type to multiple entries and add both single entries
                let single_entry = self
                    .table
                    .insert(part_checksum.clone(), DTEntry::new_multi_entry());
                if let Some(DTEntry::Single(se)) = single_entry {
                    self._add_item(part_checksum.clone(), se);
                } else {
                    panic!("Duplicate table should contain single entry at {part_checksum}");
                }
                self._add_item(part_checksum, data);
            }

            // There are multiple entries for part_checksum key
            Some(DTEntry::Multiple(_)) => {
                self._add_item(part_checksum, data);
            }

            // Table doesn't have an entry for part_checksum key
            None => {
                self.table.insert(part_checksum, DTEntry::Single(data));
            }
        }
    }

    /// Calculate full checksum and add item to multiple-item entry
    ///
    /// # Arguments
    /// * `part_checksum` - partial checksum of the item
    /// * `entry` - entry data
    fn _add_item(&mut self, part_checksum: String, entry: TableData) {
        let checksum = get_checksum(entry.path()).expect("Could not calculate checksum");
        self._add_to_mult_entries(part_checksum, checksum, entry);
    }

    /// Add item with known full checksum to multiple-item entry
    ///
    /// # Arguments
    /// * `part_checksum` - partial checksum of the item
    /// * `checksum` - checksum of the whole file in entry
    /// * `entry` - entry data
    ///
    /// # Panics
    /// Panics if the value at `partial_checksum` is not of type MultipleEntries
    fn _add_to_mult_entries(&mut self, part_checksum: String, checksum: String, entry: TableData) {
        if let Some(DTEntry::Multiple(me)) = self.table.get_mut(&part_checksum) {
            match me.hashes.get_mut(&checksum) {
                Some(v) => {
                    v.push(entry);
                }
                None => {
                    me.hashes.insert(checksum, vec![entry]);
                }
            }
        } else {
            panic!("Duplicate Table should contain Multiple entries with key:\n{part_checksum}")
        }
    }

    /// Get duplicates of entry
    ///
    /// Given partial checksum and entry data, get data of all duplicates of the entry.
    ///
    /// # Arguments
    /// `part_checksum` - Partial checksum of the file
    /// `entry` - data of the file; used to identify the file among the duplicates
    /// `entry` and `part_checksum` should be of the same file.
    pub(crate) fn get_duplicates(
        &self,
        part_checksum: &String,
        entry: &TableData,
    ) -> Result<HashSet<TableData>, &str> {
        if let Some(val) = self.table.get(part_checksum) {
            match val {
                DTEntry::Single(data) => {
                    if data == entry {
                        return Ok(HashSet::new());
                    } else {
                        return Err("There is unexpected data at {part_checksum}");
                    }
                }

                DTEntry::Multiple(MultipleEntries { hashes }) => {
                    // Find vector that contains the entry
                    for duplicates in hashes.values() {
                        if duplicates.contains(entry) {
                            let mut result: HashSet<TableData> =
                                duplicates.iter().map(|x| x.to_owned()).collect();
                            // Remove the entry itself from returned vector
                            result.remove(entry);
                            return Ok(result);
                        }
                    }
                    Err("Could not find specified entry {entry:?} in MultipleEntries at {part_checksum}")
                }
            }

        // There is no entry with this part_checksum
        } else {
            Err("There is no entry with the specified partial checksum {part_checksum:?}.")
        }
    }
}

/// Structure for DuplicateTable entries
#[derive(Debug)]
enum DTEntry {
    Single(TableData),
    Multiple(MultipleEntries),
}

impl DTEntry {
    fn new_multi_entry() -> DTEntry {
        DTEntry::Multiple(MultipleEntries {
            hashes: HashMap::new(),
        })
    }
}

/// Holds multiple items with the same partial-checksum key. Those items are sorted by full
/// checksum in addition.
#[derive(Debug)]
struct MultipleEntries {
    hashes: HashMap<String, Vec<TableData>>,
}
