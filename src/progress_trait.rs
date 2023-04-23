//! Interface for progress visualisation handlers
use std::cell::RefCell;
use std::rc::Rc;

use std::ffi::OsString;
use std::fmt::Debug;

//*********************//
// Multiline Progress  //
//*********************//
/// Multiline progress indicator trait
///
/// The progress indicator is used to simultaneously display the directory the DuDe is currently
/// searching for files and the (approximate) overall progress of duplicate file search. It is used
/// in the initial phase when the DuDe searches the added directories for duplicates.
///
/// All of the methods will generally be called multiple times.
pub trait ProgressMultiline {
    /// This method should initialise the multiline progress indicator and return a simple progress
    /// indicator for tracking the overall progress of duplicate file search.
    ///
    /// The simple progress indicator returned will indicate approximately how many files have been
    /// processed (i.e. been hashed) out of the `total_files` number of files. The simple progress
    /// indicator will be updated independently of the `ProgressAddDir` indicator via its own
    /// `update` method.
    ///
    /// The create() method of the simple progress indicator returned is not called. If you need it
    /// to be called, call it inside this method.
    ///
    /// This method can in general be called multiple times.
    ///
    /// # Arguments:
    /// * `message` - message to be displayed by the multiline indicator
    /// * `total_files` - total number of files the DuDe will process
    fn create(&mut self, message: String, total_files: u64) -> Rc<RefCell<dyn ProgressIndicator>>;

    /// Update the directory displayed by the multiline progress indicator
    fn update_dir(&self, new_dir: OsString);

    /// Finish the indicator of directory processing in `ProgressMultiline` indicator
    ///
    /// The progress indicator tracking the overall progress of of duplicate file search (that was
    /// returned by create method) is finalised separately by its own finalise() method.
    fn finalise(&self);

    /// Print some pretty debug string
    fn debug_string(&self) -> String;
}

impl Debug for dyn ProgressMultiline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Progress indicator for Adding Dir: {}", self.debug_string())
    }
}

//*********************//
// Progress indicator  //
//*********************//

/// Simple progress indicator trait
///
/// This progress indicator is used for displaying progress of most processes in the DuDe. It is
/// meant to be reused for each of the processes by repeatedly calling `create` and `finalise`
/// functions.
pub trait ProgressIndicator {
    /// Initialise the progress indicator or reinitialise it after it has been finalised.
    ///
    /// # Arguments:
    /// * `message` - message to be displayed by the indicator
    /// * `total_iterations` - total number of iterations expected
    fn create(&mut self, message: String, total_iterations: u64);

    /// Adjust the number of iterations done displayed by the progress indicator
    fn update(&self, iterations_done: u64);

    /// Finish the progress indicator. Can be followed by a call to `create` method.
    fn finalise(&self);

    /// Print some pretty debug string
    fn debug_string(&self) -> String;
}

impl Debug for dyn ProgressIndicator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Progress indicator: {}", self.debug_string())
    }
}

//*****************//
// Default structs //
//*****************//

/// Implements [`ProgressIndicator`](ProgressIndicator) without displaying anything
pub struct NoProgressIndicator {}

impl ProgressIndicator for NoProgressIndicator {
    fn create(&mut self, _message: String, _total_iterations: u64) {}
    fn update(&self, _iterations_done: u64) {}
    fn finalise(&self) {}
    fn debug_string(&self) -> String {
        "No Progress Bar".to_string()
    }
}

/// Implements [`ProgressMultiline`](ProgressMultiline) without displaying anything
pub struct NoProgressMultiline {}

impl ProgressMultiline for NoProgressMultiline {
    fn create(
        &mut self,
        _message: String,
        _total_iterations: u64,
    ) -> Rc<RefCell<dyn ProgressIndicator>> {
        Rc::new(RefCell::new(NoProgressIndicator {}))
    }
    fn update_dir(&self, _new_dir: OsString) {}
    fn finalise(&self) {}
    fn debug_string(&self) -> String {
        "No progress add dir.".to_string()
    }
}

// FIXME: Btw, why can't I implement Default for Rc<dyn ProgressIndicator> but I can implement it for
// Box<dyn ProgressIndicator>?
