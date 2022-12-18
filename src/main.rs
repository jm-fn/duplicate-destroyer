//! Duplicate Destroyer
//!
//! Command line tool that finds duplicate directories and provides their basic handling.
//!
//! # Usage
//! Scan a directory for duplicates
//! ```bash
//! duplicate_destroyer -p path/to/dir
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
//! Select action and paths. (Or press Ctrl-C to exit program.)
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
//! will delete "path/to/dir/some_dir/A" in our example. (This is not implemented yet.)

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt::Display;
use std::io;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use regex::Regex;

use duplicate_destroyer;
use duplicate_destroyer::DuplicateObject;

/// Minimum size of duplicates that is being output.
/// (DuDe still calculates the hashes and finds their duplicates, it just does not include them in
///  output)
const DEFAULT_MINIMUM_SIZE: u64 = 100;

/// CLI argument parser
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Add path to be scanned
    #[clap(short, long)]
    path: Vec<OsString>,

    /// Minimul size of duplicates considered (can have a metric prefix) [100]
    #[clap(short, long)]
    minimum_size: Option<String>,
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
}

/// Stores action and vector of file numbers of files that are acted upon
type ActionTuple = (Actions, Vec<usize>);

/// Get duplicates for user-specified directories and
///
/// The function checks CLI arguments, then finds duplicates for specified directories and prints
/// them. User can choose actions for each file in each duplicate group.
fn main() -> io::Result<()> {
    env_logger::init();

    let args = Args::parse();

    if args.path.len() == 0 {
        log::error!("Please specify at least one directory.");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No directory specified.",
        ));
    }

    // Get minimum size of elements of duplicate groups
    let mut minimum_size = DEFAULT_MINIMUM_SIZE;
    if let Some(ms) = args.minimum_size {
        minimum_size = match parse_human_readable_size(&ms) {
            None => {
                log::error!("Could not parse minimum size: {}", ms);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Bad form of minimum size: {}. Use e.g. 1k", ms),
                ));
            }
            Some(val) => val,
        }
    }

    log::info!("Minimum size of duplicates considered is: {minimum_size}");

    log::trace!("Got directories:");
    for dir in args.path.iter() {
        log::trace!("{:?}", dir)
    }

    // FIXME: This should probably have its own struct
    let mut options = HashMap::new();
    options.insert("min_size".to_string(), minimum_size);

    let duplicates = duplicate_destroyer::get_duplicates(args.path, options).unwrap();

    _print_statistics(&duplicates);

    let num_groups = duplicates.len();
    for (index, group) in duplicates.into_iter().enumerate() {
        println!("Group {}/{}", index + 1, num_groups);
        handle_group(group);
    }

    Ok(())
}

/// Print number of groups found and max space saved
///
/// # Arguments
/// * `duplicates` - Vector of all duplicate groups
fn _print_statistics(duplicates: &Vec<DuplicateObject>) {
    println!("");
    println!("{}", "-".repeat(40));
    let num_groups = duplicates.len();
    println!("Found {} groups.", num_groups);
    let max_saved_space: u64 = duplicates
        .iter()
        .map(|x| x.size * (x.duplicates.len() - 1) as u64)
        .sum();
    println!(
        "Max saved space in this iteration: {}",
        get_human_readable_size(max_saved_space)
    );
    println!("{}", "-".repeat(40));
    println!("");
}

/// Print group, get actions and execute them
///
/// # Arguments
/// * `group` - Group of duplicates to be printed and acted upon
fn handle_group(group: DuplicateObject) {
    let mut paths: Vec<_> = group.duplicates.into_iter().collect();
    paths.sort_unstable();

    // Print files in group
    let max_length = paths.iter().map(|x| x.len()).max().unwrap_or(60) + 7;
    println!("{}", "-".repeat(max_length));
    for (index, path) in paths.iter().enumerate() {
        println!("{:3}. {:?}", index, path);
    }
    println!("{}", "-".repeat(max_length));
    println!("Size: {}", get_human_readable_size(group.size));
    println!("{}", "-".repeat(11));

    println!("Select action and paths. (Or press Ctrl-C to exit program.)");
    while true {
        let (action, acted_file_nums) = get_action(paths.len());
        let acted_paths = paths
            .iter()
            .enumerate()
            .filter(|(num, _path)| acted_file_nums.contains(num))
            .map(|(_num, path)| path)
            .collect();
        execute_action(&action, acted_paths);
        match action {
            Actions::Open | Actions::OpenFolder => {}
            _ => break,
        }
        println!("Select another action.");
    }

    println!("");
}

/// Get human readable size in SI units from bytes
///
/// # Arguments
/// `size` - size in bytes
fn get_human_readable_size(size: u64) -> String {
    let mut number = size;
    for unit in ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB"] {
        if number < 1000 {
            return format!("{number}{unit}");
        }
        number = number / 1000;
    }
    format!("{number}YB")
}

/// Parse size given in SI units to bytes
///
/// # Arguments
/// * `input` - size in SI units
fn parse_human_readable_size(input: &str) -> Option<u64> {
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

/// Get action and files affected from user input
///
/// The files are represented by ints given by their order in the output (starting with 0). User
/// can input more files at once by separating them with whitespace.
///
/// # Arguments
/// * `file_num` - Number of files in the group (serves for checking )
fn get_action(file_num: usize) -> ActionTuple {
    let max_retries = 2u32;
    // FIXME: Get this string from Actions impl? //
    println!(
        "[O]pen, Open [F]older, [D]elete, ReplaceWith[H]ardlink, ReplaceWith[S]oftlink, [N]othing"
    );
    for i in 0..max_retries {
        let mut input = String::new();
        let in_result = io::stdin().read_line(&mut input);
        if let Err(e) = in_result {
            _print_action_input_err(i, max_retries, format!("Error getting input: {e}"));
            continue;
        }

        match _parse_action_input(&input.trim()) {
            Ok(val) => {
                // Check that file numbers entered are valid
                let file_max = val.1.iter().max().unwrap_or(&0);
                if *file_max < file_num {
                    // All is alright, return value
                    return val;
                } else {
                    _print_action_input_err(
                        i,
                        max_retries,
                        format!("There is no file with number {file_max}"),
                    );
                }
            }

            // Could not parse input
            Err(err) => {
                _print_action_input_err(i, max_retries, err);
            }
        };
    }
    // Did not get valid input, return default action
    (Actions::Nothing, vec![])
}

fn _print_action_input_err(iteration: u32, max_retries: u32, message: String) {
    println!("{}", message);
    if iteration < max_retries {
        println!("Try again.");
    } else {
        println!("Using the default action: Doing nothing.");
    }
}

// FIXME: Do this with some real parser...
// TODO: Add check that actions have files if applicable?
fn _parse_action_input(input: &str) -> Result<ActionTuple, String> {
    log::trace!("Got action input {input}");
    let re = Regex::new(r"(?P<action>[OFDHSN])(?P<files>(\s+\d+)*)$").unwrap();
    let captures = re.captures(&input);
    if let Some(cap) = captures {
        let action_rep = cap.name("action").unwrap().as_str();
        let action = match action_rep {
            "O" => Actions::Open,
            "F" => Actions::OpenFolder,
            "D" => Actions::Delete,
            "H" => Actions::ReplaceWithHardlink,
            "S" => Actions::ReplaceWithSoftlink,
            "N" => Actions::Nothing,
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
        return Ok((action, files));
    // Can not parse input
    } else {
        return Err(format!("Could not parse input {input}"));
    }
}

/// Execute action on all files in `files`
///
/// # Arguments
/// * `action` - Action to be taken
/// * `files` - vector of files, `action` is executed on each
fn execute_action(action: &Actions, files: Vec<&OsString>) {
    match action {
        Actions::Open => {
            for file in files {
                open_file(file);
            }
        }

        Actions::OpenFolder => {
            for file in files {
                open_containing_dir(file);
            }
        }

        Actions::Nothing => {}

        _ => {
            println!("This is not yet implemented... ");
        }
    }
}

/// Open a file using the preferred application
///
/// Uses Linux-specific `xdg-open` to open file with default application specified by desktop
// FIXME: Make this multiplatform?
fn open_file(file: &OsString) {
    log::trace!("Opening file {:?}", file);

    let file_str: String = file.to_owned().into_string().unwrap();
    let out = Command::new("xdg-open")
        .arg(file_str)
        .output()
        .expect("Could not execute xdg-open.");

    // If opening failed, print stderr
    if !out.status.success() {
        log::error!(
            "Error opening file: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Open directory containing the specified file
///
/// # Arguments
/// `file` - file, whose parent dir should be opened
fn open_containing_dir(file: &OsString) {
    let dir = Path::new(file)
        .parent()
        .expect("Could not get parent path of {data.path}")
        .as_os_str()
        .to_owned();
    open_file(&dir)
}
