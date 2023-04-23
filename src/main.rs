//! Duplicate Destroyer
//!
//! Command line tool that finds duplicate directories and provides their basic handling.
//!
//! # Usage
//! Scan a directory for duplicates
//! ```bash
//! dude -p path/to/dir
//! ```
//! Once the directory is scanned DuDe will print the duplicate groups found E.g.:
//! ```bash
//! Group 1/2
//! --------------------------------
//!   0. "path/to/dir/some_dir/A"
//!   1. "path/to/dir/other_dir/B"
//! --------------------------------
//! Size: 8kB
//! -----------
//! Select action and paths.
//! [O]pen, Open [F]older, [D]elete, ReplaceWith[H]ardlink, ReplaceWith[S]oftlink, [N]othing
//! ```
//! To act on the items found type the letter of action and file numbers. E.g.
//! ```bash
//! O 0 1
//! ```
//! will open both files.
//! ```
//! D 0
//! ```
//! will delete "path/to/dir/some_dir/A" in our example.

mod progress_bar;

use std::cmp::max;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{remove_dir_all, File};
use std::io;
use std::io::prelude::*;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::cell::RefCell;
use std::rc::Rc;

use clap::Parser;
use copy_confirmer::*;
use dialoguer::Confirm;
use minus::Pager;
use regex::Regex;
use walkdir::WalkDir;

use duplicate_destroyer::DuplicateObject;

/// Retries for input of user actions
const MAX_RETRIES: u32 = 4u32;

/// CLI argument parser
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Add path to be scanned
    #[clap(short, long, required = true)]
    path: Vec<OsString>,

    /// Minimum size of duplicates considered (can have a metric prefix) [default=100]
    #[clap(short, long)]
    minimum_size: Option<String>,

    /// Number of jobs that run simultaneously [default=0]
    #[clap(short, long)]
    jobs: Option<usize>,

    /// Output the list of duplicates to a file in json format
    #[clap(long, value_name = "FILE")]
    json_file: Option<OsString>,

    /// Disable interactive duplicate handling
    #[clap(long)]
    no_interactive: bool,
}

/// Actions possible for duplicate files
// TODO: Add Diff parent dir
#[derive(Debug)]
enum Actions {
    Open,
    OpenFolder,
    Delete,
    ReplaceWithHardlink,
    ReplaceWithSoftlink,
    Nothing,
    Quit,
}

/// Stores action, vector of paths to files to be acted upon
/// If the action changes the files, the last member is the file that should be kept unchanged.
type ActionTuple = (Actions, Vec<OsString>, Option<OsString>);

/// Get duplicates for user-specified directories and let user handle them
///
/// The function checks CLI arguments, then finds duplicates for specified directories and prints
/// them. User can choose actions for each file in each duplicate group.
fn main() -> io::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Get DuDe configuration
    let mut config: duplicate_destroyer::Config = Default::default();

    // Get minimum size of elements of duplicate groups
    if let Some(ms) = args.minimum_size {
        match _parse_human_readable_size(&ms) {
            None => {
                log::error!("Could not parse minimum size: {}", ms);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Bad form of minimum size: {}. Use e.g. 1k", ms),
                ));
            }
            Some(val) => config.set_minimum_size(val),
        }
    }

    // Get number of threads
    if let Some(num) = args.jobs {
        config.set_num_threads(max(num - 1, 0));
    }

    log::trace!("Got directories:");
    for dir in args.path.iter() {
        log::trace!("{:?}", dir)
    }

    let pb = Rc::new(RefCell::new(progress_bar::Progress::new()));
    let add_dir_pb = Rc::new(RefCell::new(progress_bar::MultiProgressBar::new()));
    config.set_multiline_progress(add_dir_pb);
    config.set_progress_indicator(pb);

    // Run Duplicate Destroyer
    let duplicates = duplicate_destroyer::get_duplicates(args.path, &config).unwrap();

    _print_statistics(&duplicates);

    // Print json results to file
    if let Some(json_file) = args.json_file {
        let serialized = serde_json::to_string_pretty(&duplicates).unwrap();
        let mut file = File::create(json_file)?;
        write!(file, "{}", serialized).expect("An error occurred when writing output to file.");
    }

    if !args.no_interactive {
        return interactive_loop(&duplicates);
    }

    Ok(())
}

/// Print all duplicate groups, let user pick actions and execute them
///
/// # Arguments
/// * `duplicates` - slice of all duplicate groups
fn interactive_loop(duplicates: &[DuplicateObject]) -> io::Result<()> {
    use Actions::*;
    let num_groups = duplicates.len();

    for (index, group) in duplicates.iter().enumerate() {
        println!("Group {}/{}", index + 1, num_groups);

        let mut paths: Vec<_> = group.duplicates.iter().map(|x| x.to_owned()).collect();
        paths.sort_unstable();

        _print_group(&paths[..], group.size);

        loop {
            let action = get_action(&paths[..])?;
            // Yay, this is ugly...
            if let Err(e) = execute_action(&action.0, action.1, action.2) {
                println!("Error running action: {}\nChoose another action.", e);
            } else if let Delete | ReplaceWithHardlink | ReplaceWithSoftlink | Nothing = action.0 {
                break; // Move to another duplicate group
            } else if let Quit = action.0 {
                return Ok(()); // Break out of the interactive loop
            }
        }
    }

    Ok(())
}

/// Get action and files affected from user input
///
/// The action is represented by a tuple - with:
/// * action.0 - member of `Actions` enum
/// * action.1 - files affected by the action
/// * action.2 - optional original file, that should stay unaffected by the action
///              (this is present only for destructive actions)
///
/// # Arguments
/// * `files` - Vector of duplicate file in a duplicate group
fn get_action(files: &[OsString]) -> io::Result<ActionTuple> {
    use Actions::*;
    println!(
        "[O]pen, Open [F]older, [D]elete, ReplaceWith[H]ardlink, ReplaceWith[S]oftlink, [N]othing, [Q]uit"
    );

    for i in 0..MAX_RETRIES {
        // get user input
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        #[allow(unused_assignments)]
        let mut action: Actions = Actions::Nothing;
        #[allow(unused_assignments)]
        let mut file_nums = vec![];

        // parse user input into Actions enum member and numbers of files
        match _parse_action_input(&input.trim().to_uppercase()) {
            Ok((new_action, new_files)) => {
                action = new_action;
                file_nums = new_files;
            }

            // Could not parse input
            Err(err) => {
                _print_action_input_err(i, MAX_RETRIES, err);
                continue;
            }
        };

        // Check that file numbers entered are valid
        let file_max = file_nums.iter().max().unwrap_or(&0);
        if !*file_max < files.len() {
            _print_action_input_err(
                i,
                MAX_RETRIES,
                format!("There is no file with number {file_max}"),
            );
            continue;
        }

        // Get paths corresponding to file numbers
        let acted_paths: Vec<_> = files
            .iter()
            .enumerate()
            .filter(|(num, _path)| file_nums.contains(num))
            .map(|(_num, path)| path.to_owned())
            .collect();

        // If we are deleting/replacing files, get a file that will not be modified
        let mut original_path: Option<OsString> = None;
        if let Delete | ReplaceWithHardlink | ReplaceWithSoftlink = action {
            if acted_paths.len() >= files.len() {
                _print_action_input_err(
                    i,
                    MAX_RETRIES,
                    "Selected destructive action for all duplicates! Please repeat selection."
                        .to_string(),
                );
                continue;
            }
            original_path =
                Some(files.iter().find(|x| !acted_paths.contains(x)).unwrap().to_owned());
        }
        return Ok((action, acted_paths, original_path));
    }
    // Did not get valid input, return default action
    Err(io::Error::new(io::ErrorKind::InvalidInput, "Failed to parse user input."))
}

/// Execute action on all files in `files`
///
/// # Arguments
/// * `action` - Action to be taken
/// * `files` - vector of files, `action` is executed on each
fn execute_action(
    action: &Actions,
    files: Vec<OsString>,
    original_path: Option<OsString>,
) -> io::Result<()> {
    match action {
        Actions::Open => {
            for file in files {
                open_file(&file)?;
            }
        }

        Actions::OpenFolder => {
            for file in files {
                open_containing_dir(&file)?;
            }
        }

        Actions::Nothing => {}
        Actions::Quit => std::process::exit(0),
        Actions::Delete => {
            if let Some(original) = original_path {
                for file in files {
                    delete_dir(&file, &original)?;
                }
            } else {
                panic!("There is no original path for delete action.")
            }
        }

        _ => {
            println!("This is not yet implemented... ");
        }
    }

    Ok(())
}

/// Open a file using the preferred application
///
/// Uses Linux-specific `xdg-open` to open file with default application specified by desktop
// FIXME: Make this multiplatform?
fn open_file(file: &OsString) -> io::Result<()> {
    log::trace!("Opening file {:?}", file);

    let file_str: String = file.to_owned().into_string().unwrap();
    let out = Command::new("xdg-open").arg(file_str).output()?;

    // If opening failed, print stderr
    if !out.status.success() {
        log::error!("Error opening file: {}", String::from_utf8_lossy(&out.stderr));
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Could not open file {file:?} with xdg-open. Got status {}",
                out.status.code().unwrap_or(0)
            ),
        ));
    }
    Ok(())
}

/// Open directory containing the specified file
///
/// # Arguments
/// `file` - file, whose parent dir should be opened
fn open_containing_dir(file: &OsString) -> io::Result<()> {
    let dir = Path::new(file)
        .parent()
        .expect("Could not get parent path of {data.path}")
        .as_os_str()
        .to_owned();
    open_file(&dir)
}

/// Delete `deleted` dir
///
/// First confirms that user truly wants to delete the directory, that all the files in
/// `deleted` dir are present in another (`original`) dir and that the directories share no inodes.
///
/// # Arguments
/// * `deleted` - deleted directory
/// * `original` - directory that should contain all the files of `deleted`
fn delete_dir(deleted: &OsString, original: &OsString) -> io::Result<()> {
    // Prompt user for confirmation
    if !Confirm::new()
        .with_prompt(format!("Do you want to delete {:?}", deleted))
        .wait_for_newline(true)
        .interact()?
    {
        eprintln!("Abandoning deletion...");
        return Ok(());
    }

    // Check that original contains all files of deleted and that they share no inodes
    if !_verify_copy(original, deleted)? {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Could not delete {:?}, could not verify that it is indeed copy", deleted),
        ));
    }

    eprintln!("Deleting {:?}", deleted);
    remove_dir_all(deleted)?;
    Ok(())
}

// ******************//
//  Helper functions //
// ******************//

/// Print number of groups found and max space saved
///
/// # Arguments
/// * `duplicates` - Vector of all duplicate groups
fn _print_statistics(duplicates: &Vec<DuplicateObject>) {
    println!();
    println!("{}", "-".repeat(40));
    let num_groups = duplicates.len();
    println!("Found {} groups.", num_groups);
    let max_saved_space: u64 =
        duplicates.iter().map(|x| x.size * (x.duplicates.len() - 1) as u64).sum();
    println!("Max saved space in this iteration: {}", _get_human_readable_size(max_saved_space));
    println!("{}", "-".repeat(40));
    println!();
}

// FIXME: Do this with some real parser...
// TODO: Add check that actions have files if applicable?
/// Parse user input string into action and file numbers
///
/// Returns a tuple of Actions enum member and a vector of file numbers
fn _parse_action_input(input: &str) -> Result<(Actions, Vec<usize>), String> {
    log::trace!("Got action input {input}");
    let re = Regex::new(r"(?P<action>[OFDHSNQ])(?P<files>(\s+\d+)*)$").unwrap();
    let captures = re.captures(input);
    if let Some(cap) = captures {
        let action_rep = cap.name("action").unwrap().as_str();
        let action = match action_rep {
            "O" => Actions::Open,
            "F" => Actions::OpenFolder,
            "D" => Actions::Delete,
            "H" => Actions::ReplaceWithHardlink,
            "S" => Actions::ReplaceWithSoftlink,
            "N" => Actions::Nothing,
            "Q" => Actions::Quit,
            &_ => panic!("Error parsing"),
        };
        // Get parsed files
        let mut files: Vec<usize> = vec![];
        if let Some(files_rep) = cap.name("files") {
            files = files_rep
                .as_str()
                .split_whitespace()
                .map(|s| s.parse().expect("Parsing error"))
                .collect();
        }
        Ok((action, files))
    // Can not parse input
    } else {
        Err(format!("Could not parse input \"{input}\"."))
    }
}

/// Print error if the user entered action in wrong format
fn _print_action_input_err(iteration: u32, max_retries: u32, message: String) {
    println!("{}", message);
    if iteration < max_retries {
        println!("Try again:");
    }
}

/// Get human readable size in SI units from bytes
///
/// # Arguments
/// `size` - size in bytes
fn _get_human_readable_size(size: u64) -> String {
    let mut number = size;
    for unit in ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB"] {
        if number < 1000 {
            return format!("{number}{unit}");
        }
        number /= 1000;
    }
    format!("{number}YB")
}

/// Print group info
fn _print_group(paths: &[OsString], size: u64) {
    // Print files in group
    let max_length = paths.iter().map(|x| x.len()).max().unwrap_or(60) + 7;
    println!("{}", "-".repeat(max_length));
    for (index, path) in paths.iter().enumerate() {
        println!("{:3}. {:?}", index, path);
    }
    println!("{}", "-".repeat(max_length));
    println!("Size: {}", _get_human_readable_size(size));
    println!("{}", "-".repeat(11));
}

/// Parse size given in SI units to bytes
///
/// # Arguments
/// * `input` - size in SI units
fn _parse_human_readable_size(input: &str) -> Option<u64> {
    let mut result = None;

    let re = Regex::new(r"(?P<value>\d+)(?P<prefix>[kMGTPE])?$").unwrap();
    let captures = re.captures(input);
    if let Some(cap) = captures {
        let cap_value: u64 = cap.name("value").unwrap().as_str().parse().unwrap();
        let multiplier: u64 = match cap.name("prefix").map(|x| x.as_str()) {
            None => 1,
            Some("k") => 1000,
            Some("M") => 1_000_000,
            Some("G") => 10u64.pow(9),
            Some("T") => 10u64.pow(12),
            Some("P") => 10u64.pow(15),
            Some("E") => 10u64.pow(18),
            Some(err) => panic!("There should not be {err} in captured prefixes."),
        };
        result = Some(cap_value * multiplier);
    }

    result
}

/// Verify that it is safe to delete copy
///
/// Verifies that all file in `copy` dir are present in `original` dir and that the two directories
/// share no inodes.
///
/// # Arguments
/// * `original` - directory that will be left unchanged in destructive operations
/// * `copy` - directory that will be changed in destructive operation
fn _verify_copy(original: &OsString, copy: &OsString) -> io::Result<bool> {
    // verify that copy contains all files of original dir
    eprintln!("Checking that all files in {:?} are duplicates:", copy);
    let cc = CopyConfirmer::new(1);
    let cc_result = cc.compare(original.to_owned(), vec![copy.to_owned()]).unwrap();
    if let ConfirmerResult::MissingFiles(missing_files) = cc_result {
        // Print out all missing files
        let mut file_text = format!("Missing files from {:?}: (Press q to quit)\n", copy);
        for file in missing_files {
            file_text.push_str(&format!("{:?}", file));
        }
        _print_to_pager(file_text);

        return Ok(false);
    }

    // Verify that the copy shares no inodes with original dir
    let origin_inodes: HashSet<_> = WalkDir::new(original)
        .into_iter()
        .filter_map(|x| x.ok()) // FIXME: maybe let this panic instead to avoid missing inodes?
        .map(|x| x.metadata())
        .filter_map(|x| x.ok())
        .map(|x| x.ino())
        .collect();

    let copy_inodes: HashSet<_> = WalkDir::new(copy)
        .into_iter()
        .filter_map(|x| x.ok())
        .map(|x| x.metadata())
        .filter_map(|x| x.ok())
        .map(|x| x.ino())
        .collect();

    if !copy_inodes.is_disjoint(&origin_inodes) {
        eprintln!(
            "There are some files in {:?} and {:?} sharing inodes. I will not to delete {:?}",
            original, copy, copy
        );
        return Ok(false);
    }

    Ok(true)
}

/// Print text to static pager
fn _print_to_pager(text: String) {
    // FIXME: somehow this interferes with std::fmt::Write, put this to the top of file
    use std::fmt::Write;

    let mut output = Pager::new();
    write!(output, "{}", text).expect("Could not write to pager");
    minus::page_all(output).expect("Could not write to pager.");
}
