/// 

use std::collections::HashSet;
use std::io;
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;

use copy_confirmer::*;
use walkdir::WalkDir;
use minus::Pager;

/// Print text to static pager
pub fn print_to_pager(text: String) {
    // FIXME: somehow this interferes with std::fmt::Write, put this to the top of file
    use std::fmt::Write;

    let mut output = Pager::new();
    write!(output, "{}", text).expect("Could not write to pager");
    minus::page_all(output).expect("Could not write to pager.");
}

/// Verify that it is safe to delete copy
///
/// Verifies that all file in `copy` dir are present in `original` dir and that the two directories
/// share no inodes.
///
/// # Arguments
/// * `original` - directory that will be left unchanged in destructive operations
/// * `copy` - directory that will be changed in destructive operation
pub fn verify_copy(original: &OsString, copy: &OsString) -> io::Result<bool> {
    // verify that copy contains all files of original dir
    eprintln!("Checking that all files in {:?} are duplicates:", copy);
    let cc = CopyConfirmer::new(1);
    let cc_result = cc.compare(original, &[copy]).unwrap();
    if let ConfirmerResult::MissingFiles(missing_files) = cc_result {
        // Print out all missing files
        let mut file_text = format!("Missing files from {:?}: (Press q to quit)\n", copy);
        for file in missing_files {
            file_text.push_str(&format!("{:?}", file));
        }
        print_to_pager(file_text);

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

