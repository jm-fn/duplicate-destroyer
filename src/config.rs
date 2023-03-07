//! Configuration of duplicate destroyer
//!
//! This module provides the structure that contains all configuration of duplicate destroyer.

/// Stores all configuration of Duplicate Destroyer
#[derive(Default, Clone)]
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
}
