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
mod helper_functions;
mod actions;

use std::cell::RefCell;
use std::cmp::max;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::rc::Rc;

use clap::Parser;
use regex::Regex;

use duplicate_destroyer::DuplicateObject;
use actions::*;


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
        match parse_human_readable_size(&ms) {
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

    print_statistics(&duplicates);

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
    let num_groups = duplicates.len();

    for (index, group) in duplicates.iter().enumerate() {
        println!("Group {}/{}", index + 1, num_groups);

        let mut paths: Vec<_> = group.duplicates.iter().map(|x| x.to_owned()).collect();
        paths.sort_unstable();

        print_group(&paths[..], group.size);

        loop {
            let action = Actions::get_from_input(&paths[..])?;
            if let Err(e) = action.execute() {
                println!("Error running action: {}\nChoose another action.", e);
            } else if !action.should_get_another() {
                break; // Move to another duplicate group
            }
        }
    }

    Ok(())
}

// ******************//
//  Helper functions //
// ******************//

/// Print number of groups found and max space saved
///
/// # Arguments
/// * `duplicates` - Vector of all duplicate groups
fn print_statistics(duplicates: &Vec<DuplicateObject>) {
    println!();
    println!("{}", "-".repeat(40));
    let num_groups = duplicates.len();
    println!("Found {} groups.", num_groups);
    let max_saved_space: u64 =
        duplicates.iter().map(|x| x.size * (x.duplicates.len() - 1) as u64).sum();
    println!("Max saved space in this iteration: {}", get_human_readable_size(max_saved_space));
    println!("{}", "-".repeat(40));
    println!();
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
        number /= 1000;
    }
    format!("{number}YB")
}

/// Print group info
fn print_group(paths: &[OsString], size: u64) {
    // Print files in group
    let max_length = paths.iter().map(|x| x.len()).max().unwrap_or(60) + 7;
    println!("{}", "-".repeat(max_length));
    for (index, path) in paths.iter().enumerate() {
        println!("{:3}. {:?}", index, path);
    }
    println!("{}", "-".repeat(max_length));
    println!("Size: {}", get_human_readable_size(size));
    println!("{}", "-".repeat(11));
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
