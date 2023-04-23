use std::cell::RefCell;
use std::rc::Rc;

use crate::OsString;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use duplicate_destroyer::{ProgressIndicator, ProgressMultiline};

/// Struct with one progress bar for overall progress of search for file duplicates and one spinner
/// to display the directory currently processed.
pub struct MultiProgressBar {
    multiprogress: MultiProgress,
    dir_spinner: ProgressBar,
}

impl MultiProgressBar {
    /// Constructor.
    pub fn new() -> Self {
        Self { multiprogress: MultiProgress::new(), dir_spinner: ProgressBar::new_spinner() }
    }
}

impl ProgressMultiline for MultiProgressBar {
    /// Create a new multiprogress with one directory spinner and one overall progress bar
    fn create(
        &mut self,
        _message: String,
        total_iterations: u64,
    ) -> Rc<RefCell<dyn ProgressIndicator>> {
        // Set slower update frequency to make the dir print less overwhelming
        self.multiprogress = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(5));
        // Dir spinner style
        let spinner_style = ProgressStyle::with_template("{spinner} {wide_msg}")
            .unwrap()
            .tick_strings(&["▹▹▹▹", "▸▹▹▹", "▹▸▹▹", "▹▹▸▹", "▹▹▹▸", "▪▪▪▪"]);
        let dir_spinner = ProgressBar::new_spinner().with_style(spinner_style);
        self.dir_spinner = self.multiprogress.add(dir_spinner);

        // overall progress style
        let pb_style = ProgressStyle::with_template(
            "{msg} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}",
        )
        .unwrap()
        .progress_chars("##-");
        let checksum_pb = ProgressBar::new(total_iterations)
            .with_style(pb_style)
            .with_message("Calculating hashes:");

        // return the overall progress bar
        let mut out_progress = Progress::new();
        out_progress.set_progress_bar(self.multiprogress.add(checksum_pb));
        Rc::new(RefCell::new(out_progress))
    }

    /// Set the dir displayed by the dir spinner
    fn update_dir(&self, new_dir: OsString) {
        self.dir_spinner.set_message(format!("Checking directories: {:?}", new_dir));
        self.dir_spinner.tick();
    }

    /// Finalise dir spinner
    fn finalise(&self) {
        self.dir_spinner.finish_with_message("Checking directories: Done");
    }

    // FIXME: Print something useful?
    /// Print some info
    fn debug_string(&self) -> String {
        "AddDirBar".into()
    }
}

/// Struct holding simple progress bar/spinner
///
/// We either set a progress bar in the create method of MultiProgressBar or set a progress spinner
/// in its own create method.
pub struct Progress {
    progress_bar: ProgressBar,
}

impl Progress {
    /// Constructor. Yay...
    pub fn new() -> Self {
        Self { progress_bar: ProgressBar::new(0) }
    }

    /// Set the progress bar to `new_pb`
    pub fn set_progress_bar(&mut self, new_pb: ProgressBar) {
        self.progress_bar = new_pb;
    }
}

impl ProgressIndicator for Progress {
    /// Create simple progress indicator with spinner and `message`.
    fn create(&mut self, message: String, _total_iterations: u64) {
        let spinner_style = ProgressStyle::with_template("{spinner} {wide_msg}")
            .unwrap()
            .tick_strings(&["▹▹▹▹", "▸▹▹▹", "▹▸▹▹", "▹▹▸▹", "▹▹▹▸", "▪▪▪▪"]);
        self.progress_bar =
            ProgressBar::new_spinner().with_style(spinner_style).with_message(message);
    }

    /// Update position in progress indicator to `iterations_done` or spin spinner.
    fn update(&self, iterations_done: u64) {
        self.progress_bar.set_position(iterations_done)
    }

    /// Finish the progress bar/spinner.
    fn finalise(&self) {
        self.progress_bar.finish()
    }

    // FIXME: Print something useful?
    /// Print some info
    fn debug_string(&self) -> String {
        "Progress Bar".to_string()
    }
}
