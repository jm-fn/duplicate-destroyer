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
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time;

use threadpool::ThreadPool;

use crate::checksum::get_checksum;
use crate::dir_tree::TableData;
use crate::{NoProgressIndicator, ProgressIndicator};

type PartialChecksum = String;
type Checksum = String;

const HUNDRED_MILIS: time::Duration = time::Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct DuplicateTable {
    table: HashMap<String, DTEntry>,
    threadpool: Option<ThreadPool>,
    checksum_rx: Receiver<(PartialChecksum, Checksum, TableData)>,
    checksum_tx: Sender<(PartialChecksum, Checksum, TableData)>,
    job_counter: u32, // Counts if DT got a checksum for each job created
    file_count: u64,
    multithreaded: bool,
    progress_indicator: Rc<RefCell<dyn ProgressIndicator>>,
}

impl DuplicateTable {
    /// Create new empty DuplicateTable
    ///
    /// # Arguments
    /// * `num_threads` - number of threads to be created by duplicate table
    pub(crate) fn new(num_threads: usize) -> Self {
        // Create threadpool if num_threads > 0
        let mut threadpool = None;
        let mut multithreaded = false;

        if num_threads != 0 {
            threadpool = Some(ThreadPool::new(num_threads));
            multithreaded = true;
        }

        let (checksum_tx, checksum_rx) = channel::<(PartialChecksum, Checksum, TableData)>();

        let progress_indicator = Rc::new(RefCell::new(NoProgressIndicator {}));

        DuplicateTable {
            table: HashMap::new(),
            threadpool,
            multithreaded,
            checksum_rx,
            checksum_tx,
            job_counter: 0,
            file_count: 0,
            progress_indicator,
        }
    }

    pub(crate) fn set_progress_indicator(
        &mut self,
        progress_indicator: Rc<RefCell<dyn ProgressIndicator>>,
    ) {
        self.progress_indicator = progress_indicator;
    }

    /// Adds a file to duplicate table.
    ///
    /// # Arguments
    /// `part_checksum` - partial checksum of the file
    /// `data` - table data corresponding to the file
    pub(crate) fn register_item(&mut self, part_checksum: String, data: TableData) {
        // Stop early if any thread panicked
        if self.multithreaded && self.threadpool.as_ref().unwrap().panic_count() > 0 {
            panic!("There is at least one panicked checksum thread.");
        }

        self.file_count += 1;

        match self.table.get(&part_checksum) {
            // There is single entry for part_checksum key
            Some(DTEntry::Single(_)) => {
                // change value type to multiple entries and add both single entries
                let single_entry =
                    self.table.insert(part_checksum.clone(), DTEntry::new_multi_entry());
                if let Some(DTEntry::Single(se)) = single_entry {
                    self.add_item(part_checksum.clone(), se);
                } else {
                    panic!("Duplicate table should contain single entry at {part_checksum}");
                }
                self.add_item(part_checksum, data);
            }

            // There are multiple entries for part_checksum key
            Some(DTEntry::Multiple(_)) => {
                self.add_item(part_checksum, data);
            }

            // Table doesn't have an entry for part_checksum key yet
            None => {
                self.table.insert(part_checksum, DTEntry::Single(data));
                self.progress_indicator.borrow().update(self.file_count - self.job_counter as u64);
            }
        }
    }

    /// Makes sure the table is finished if multithreading is on
    pub(crate) fn finalise(&mut self) {
        if self.multithreaded {
            log::debug!("Waiting for jobs in duplicate table.");
            // Wait for all jobs to finish
            let threadpool = self.threadpool.as_ref().unwrap();
            let mut num_not_done = threadpool.active_count() + threadpool.queued_count();
            while num_not_done > 0 {
                num_not_done = threadpool.active_count() + threadpool.queued_count();
                self.progress_indicator.borrow().update(self.file_count - num_not_done as u64);
                log::info!("Tracking progress.");
                thread::sleep(2 * HUNDRED_MILIS);
            }

            log::debug!("All jobs in dupllicate table finished");

            // Panic if any thread panicked
            if self.threadpool.as_ref().unwrap().panic_count() > 0 {
                panic!("There is at least one panicked checksum thread.");
            }

            // Add all calculated checksums to dupl. table
            for (part_checksum, checksum, entry) in
                self.checksum_rx.try_iter().collect::<Vec<(PartialChecksum, Checksum, TableData)>>()
            {
                log::trace!("Adding {:?} to mult entries", entry.path());
                self.add_to_mult_entries(part_checksum, checksum, entry);
            }
            log::trace!("Done adding checksums to duplicate table.");

            self.progress_indicator.borrow().finalise();

            // Panic if we are missing any checksum
            if self.job_counter > 0 {
                panic!("There were more jobs created ")
            }
        }
    }

    /// Calculate full checksum and add item to multiple-item entry
    ///
    /// If the table is multithreaded creates a job to calculate the checksum, otherwise calculates
    /// checksum and adds the entry to duplicate table.
    ///
    /// # Arguments
    /// * `part_checksum` - partial checksum of the item
    /// * `entry` - entry data
    fn add_item(&mut self, part_checksum: String, entry: TableData) {
        if self.multithreaded {
            self.add_job(part_checksum, entry);
        } else {
            let checksum = get_checksum(entry.path()).expect("Could not calculate checksum");
            self.add_to_mult_entries(part_checksum, checksum, entry);
        }
    }

    /// Add a job to calculate the checksum of the entry to the threadpool
    ///
    /// # Arguments
    /// * `part_checksum` - partial checksum of the item
    /// * `entry` - entry data
    fn add_job(&mut self, part_checksum: String, entry: TableData) {
        log::debug!("Adding job for {:?}", entry.path());
        self.job_counter += 1;
        let checksum_tx = self.checksum_tx.clone();
        self.threadpool.as_ref().unwrap().execute(move || {
            let checksum = get_checksum(entry.path()).expect("Could not calculate checksum");
            checksum_tx.send((part_checksum, checksum, entry)).expect("Could not send data.");
        })
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
    fn add_to_mult_entries(&mut self, part_checksum: String, checksum: String, entry: TableData) {
        if self.multithreaded {
            self.job_counter -= 1;
        }
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
        self.progress_indicator.borrow().update(self.file_count - self.job_counter as u64);
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
        part_checksum: &str,
        entry: &TableData,
    ) -> Result<HashSet<TableData>, &str> {
        if let Some(val) = self.table.get(part_checksum) {
            match val {
                DTEntry::Single(data) => {
                    if data == entry {
                        Ok(HashSet::new())
                    } else {
                        Err("There is unexpected data at {part_checksum}")
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
        DTEntry::Multiple(MultipleEntries { hashes: HashMap::new() })
    }
}

/// Holds multiple items with the same partial-checksum key. Those items are sorted by full
/// checksum in addition.
#[derive(Debug)]
struct MultipleEntries {
    hashes: HashMap<String, Vec<TableData>>,
}
