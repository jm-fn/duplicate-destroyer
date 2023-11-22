/// Actions that can be performed on each group found by DuDe

use crate::helper_functions::*;

use std::ffi::OsString;
use std::fs::{remove_dir_all, remove_file};
use std::io;
use std::path::Path;
use std::process::Command;

use copy_confirmer::*;
use dialoguer::Confirm;
use regex::Regex;

/// Retries for input of user actions
const MAX_RETRIES: u32 = 4;


/// Actions possible for duplicate groups
///
/// All actions except `Nothing` and `Quit` contain vector of paths the action should be taken on.
/// Destructive actions (Delete, ReplaceWithHardlink and ReplaceWithSoftlink) also contain a path
/// that will not be changed to ensure that at least one path stays intact.
// TODO: Add Diff parent dir
#[derive(Debug)]
pub enum Actions {
    Open(Vec<OsString>),
    OpenFolder(Vec<OsString>),
    Delete(Vec<OsString>, OsString),
    ReplaceWithHardlink(Vec<OsString>, OsString),
    ReplaceWithSoftlink(Vec<OsString>, OsString),
    Nothing,
    Quit,
}

enum LinkType {
    HardLink,
    SoftLink,
}

impl Actions {
    pub fn execute(&self) -> io::Result<()> {
        use Actions::*; 

        match self {
            Delete(files, original) => {
                for file in files {
                    delete_dir(file, original)?;
                }
            }

            Nothing => {}

            Open(files) => {
                for file in files {
                    open_file(file)?;
                }
            }

            OpenFolder(files) => {
                for file in files {
                    open_containing_dir(file)?;
                }
            }

            ReplaceWithHardlink(files, original) => {
                for file in files {
                    replace_with_link(file, original, LinkType::HardLink)?;
                }
            }

            ReplaceWithSoftlink(files, original) => {
                for file in files {
                    replace_with_link(file, original, LinkType::SoftLink)?;
                }
            }

            Quit => std::process::exit(0),
        }

        Ok(())

    }

    /// Returns true if action can be followed by another action
    pub fn should_get_another(&self) -> bool {
        use Actions::*;

        matches!(self, Open(_) | OpenFolder(_))
    }

    /// Get action and files affected from user input
    ///
    /// # Arguments
    /// * `files` - Vector of duplicate files in a duplicate group
    pub fn get_from_input(files: &[OsString]) -> io::Result<Actions> {
        use Actions::*;

        println!(
            "[O]pen, Open [F]older, [D]elete, ReplaceWith[H]ardlink, ReplaceWith[S]oftlink, [N]othing, [Q]uit"
        );

        for i in 0..MAX_RETRIES {
            // get user input
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            #[allow(unused_assignments)]
            let mut file_nums = vec![];
            #[allow(unused_assignments)]
            let mut action_rep = String::new();

            // parse user input into Actions enum member and numbers of files
            match Self::parse_action_input(&input.trim().to_uppercase()) {
                Ok((new_action, new_files)) => {
                    action_rep = new_action;
                    file_nums = new_files;
                }

                // Could not parse input
                Err(err) => {
                    Self::print_action_input_err(i, &err);
                    continue;
                }
            };

            // Check that user input files for actions that require them
            if let "O" | "F" | "D" | "S" | "H" = action_rep.as_str() {
                if file_nums.is_empty(){
                    Self::print_action_input_err(i, "Select at least one file for this action.")
                }
            }

            // Check that file numbers entered are valid
            let file_max = file_nums.iter().max().unwrap_or(&0);
            if *file_max > files.len() {
                Self::print_action_input_err(
                    i,
                    &format!("There is no file with number {file_max}"),
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
            if let "D" | "S" | "H" = action_rep.as_str() {
                if acted_paths.len() >= files.len() {
                    Self::print_action_input_err(
                        i,
                        "Selected destructive action for all duplicates! Please repeat selection."
                    );
                    continue;
                }
                original_path =
                    Some(files.iter().find(|x| !acted_paths.contains(x)).unwrap().to_owned());
            }

            // Create action
            let action = match action_rep.as_str() {
                "D" => Delete(acted_paths, original_path.unwrap()),
                "S" => ReplaceWithSoftlink(acted_paths, original_path.unwrap()),
                "H" => ReplaceWithHardlink(acted_paths, original_path.unwrap()),
                "O" => Open(acted_paths),
                "F" => OpenFolder(acted_paths),
                "Q" => Quit,
                "N" => Nothing, 
                &_ => panic!("Error parsing user input.")
                
            };
            
            return Ok(action);
        }
        // Did not get valid input, return default action
        Err(io::Error::new(io::ErrorKind::InvalidInput, "Failed to parse user input."))
    }

    // FIXME: Do this with some real parser...
    /// Parse user input string into action and file numbers
    ///
    /// Returns a tuple of Actions enum member and a vector of file numbers
    fn parse_action_input(input: &str) -> Result<(String, Vec<usize>), String> {
        log::trace!("Got action input {input}");
        let re = Regex::new(r"(?P<action>[OFDHSNQ])(?P<files>(\s+\d+)*)$").unwrap();
        let captures = re.captures(input);
        if let Some(cap) = captures {
            let action_str = cap.name("action").unwrap().as_str().to_owned();
            // Get parsed files
            let mut files: Vec<usize> = vec![];
            if let Some(files_rep) = cap.name("files") {
                files = files_rep
                    .as_str()
                    .split_whitespace()
                    .map(|s| s.parse().expect("Parsing error"))
                    .collect();
            }
            Ok((action_str, files))
        // Can not parse input
        } else {
            Err(format!("Could not parse input \"{input}\"."))
        }
    }

    /// Print error if the user entered action in wrong format
    fn print_action_input_err(iteration: u32, message: &str) {
        println!("{}", message);
        if iteration < MAX_RETRIES {
            println!("Try again:");
        } else {
            println!("Let's move to another group instead...");
        }
    }

}

/********************/
/* Action functions */
/********************/

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
        .interact()
        .expect("Could not show dialogue.")
    {
        println!("Abandoning deletion...");
        return Ok(());
    }

    // Check that original contains all files of deleted and that they share no inodes
    if !verify_copy(original, deleted)? {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Could not delete {:?}, could not verify that it is indeed copy", deleted),
        ));
    }

    println!("Deleting {:?}", deleted);
    if Path::new(&deleted).is_dir() {
        remove_dir_all(deleted)?;
    } else {
        remove_file(deleted)?;
    }
    Ok(())
}

/// Replace files in `replaced` with hard links to files in `original`
///
/// Confirms that user really wants to replace all files with hard links and that all files are in
/// the `original` dir and then replaces all the files with hardlinks to their duplicates
///
/// # Arguments
/// * `replaced` - folder whose content should be replaced with hardlinks
/// * `original` - folder whose contents should be kept
// FIXME: Make this multiplatform?
fn replace_with_link(
    replaced: &OsString,
    original: &OsString,
    link_type: LinkType,
) -> io::Result<()> {
    #[allow(unused_assignments)]
    let mut prompt = String::new();
    if let LinkType::HardLink = link_type {
        prompt = format!("Do you want to replace all contents of {:?} with hard links?", replaced);
    } else {
        prompt = format!("Do you want to replace all contents of {:?} with soft links?", replaced);
    }
    // Prompt user for confirmation
    if !Confirm::new()
        .with_prompt(prompt)
        .wait_for_newline(true)
        .interact()
        .expect("Could not show dialogue.")
    {
        println!("Abandoning replacement...");
        return Ok(());
    }

    // Check that original contains all files of replaced folder
    println!("Checking that all files in {:?} are duplicates:", replaced);
    let cc = CopyConfirmer::new(1);
    let cc_result = cc.compare(replaced.to_owned(), &[original.to_owned()]).unwrap();
    match cc_result {
        // If there are some files missing in original abort
        ConfirmerResult::MissingFiles(missing_files) => {
            // Print out all missing files
            let mut file_text = format!("Missing files from {:?}: (Press q to quit)\n", replaced);
            for file in missing_files {
                file_text.push_str(&format!("{:?}", file));
            }
            print_to_pager(file_text);

            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Could not replace {:?} with links. Could not verify it is indeed copy",
                    replaced
                ),
            ));
        }

        ConfirmerResult::Ok(found_files) => {
            // src_paths are files in `replaced` directory, dest_paths are their duplicates in
            // `original` directory
            println!("Done.");
            println!("Replacing all files at {:?} with links.", replaced);
            for FileFound { src_paths, dest_paths } in found_files.values() {
                for path in src_paths {
                    remove_file(path)?;
                    if let LinkType::HardLink = link_type {
                        std::fs::hard_link(&dest_paths[0], path)?;
                    } else {
                        std::os::unix::fs::symlink(&dest_paths[0], path)?;
                    }
                }
            }
        }
    }

    Ok(())
}

