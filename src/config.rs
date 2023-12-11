//! Configuration of duplicate destroyer
//!
//! This module provides the structure that contains all configuration of duplicate destroyer.
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    HashAlgorithm, NoProgressIndicator, NoProgressMultiline, ProgressIndicator, ProgressMultiline,
};

/// Stores all configuration of Duplicate Destroyer
#[derive(Default)]
pub struct Config {
    /// Minimum size of elements in [`DuplicateObjects`](crate::DuplicateObject) returned. [default = 100]
    ///
    /// This option has almost no bearing on the speed of Duplicate Destroyer, only on the output
    /// size. The DuDe calculates checksums of files smaller than `minimum_size`, since the larger
    /// directories might be composed of these smaller files and if we disregarded them, we could
    /// lose some small but important data.
    pub minimum_size: Option<u64>,

    /// Number of threads spawned for calculating the checksums of files [default = 0]
    pub num_threads: Option<usize>,

    /// Simple progress indicator.
    ///
    /// To add a progress indicator to the DuDe, set to a trait object implementing the
    /// [`ProgressIndicator`](crate::progress_trait::ProgressIndicator) trait.
    /// [default = [`NoProgressIndicator`](crate::progress_trait::NoProgressIndicator)]
    pub progress_indicator: Option<Rc<RefCell<dyn ProgressIndicator>>>,

    /// Multiline progress indicator.
    ///
    /// To add a progress indicator for indicating the progress of duplicate file search add a
    /// trait object implementing the
    /// [`ProgressMultiline`](crate::progress_trait::ProgressMultiline) trait.
    /// [default = [`NoProgressMultiline`](crate::progress_trait::NoProgressAddDir)]
    pub progress_multiline: Option<Rc<RefCell<dyn ProgressMultiline>>>,

    /// Hashing algorithm used to compare the files [default = Blake3]
    pub hash_algorithm: Option<HashAlgorithm>,
}

impl Config {
    /// Set [`minimum_size`](Config::minimum_size)
    pub fn set_minimum_size(&mut self, min_size: u64) {
        self.minimum_size = Some(min_size);
    }

    /// Get [`minimum_size`](Config::minimum_size)
    pub fn get_minimum_size(&self) -> u64 {
        self.minimum_size.unwrap_or(100)
    }

    /// Set [`num_threads`](Config::num_threads)
    pub fn set_num_threads(&mut self, num_threads: usize) {
        self.num_threads = Some(num_threads);
    }

    /// Get [`num_threads`](Config::num_threads)
    pub fn get_num_threads(&self) -> usize {
        self.num_threads.unwrap_or(0)
    }

    /// Set [`progress_indicator`](Config::progress_indicator)
    pub fn set_progress_indicator(
        &mut self,
        progress_indicator: Rc<RefCell<dyn ProgressIndicator>>,
    ) {
        self.progress_indicator = Some(progress_indicator);
    }

    /// Get [`progress_indicator`](Config::progress_indicator)
    pub fn get_progress_indicator(&self) -> Rc<RefCell<dyn ProgressIndicator>> {
        if let Some(ref pi) = self.progress_indicator {
            Rc::clone(pi)
        } else {
            Rc::new(RefCell::new(NoProgressIndicator {}))
        }
    }

    /// Set [`multiline_progress`](Config::multiline_progress)
    pub fn set_multiline_progress(
        &mut self,
        progress_indicator: Rc<RefCell<dyn ProgressMultiline>>,
    ) {
        self.progress_multiline = Some(progress_indicator);
    }

    /// Get [`multiline_progress`](Config::multiline_progress)
    pub fn get_multiline_progress(&self) -> Rc<RefCell<dyn ProgressMultiline>> {
        if let Some(ref pm) = self.progress_multiline {
            Rc::clone(pm)
        } else {
            Rc::new(RefCell::new(NoProgressMultiline {}))
        }
    }

    /// Set [`hash_algorithm`](Config::hash_algorithm)
    pub fn set_hash_algorithm(&mut self, hash_algorithm: HashAlgorithm) {
        self.hash_algorithm = Some(hash_algorithm);
    }

    /// Get [`hash_algorithm`](Config::hash_algorithm)
    pub fn get_hash_algorithm(&self) -> HashAlgorithm {
        self.hash_algorithm.unwrap_or(HashAlgorithm::Blake2)
    }
}
