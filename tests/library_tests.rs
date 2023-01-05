use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{File, DirBuilder};
use std::path::Path;
use std::io::{self, Write};
use std::process::Command;

use tempdir::TempDir;

use duplicate_destroyer;
use duplicate_destroyer::DuplicateObject;

fn write_file(path: &Path, contents: &str) {
    let mut file = File::create(path).expect("Could not create a file.");
    writeln!(file, "{}", contents);
}

#[test]
/// Create a directory structure with the schema
/// tempdir
/// ├── A
/// │   ├── a.txt
/// │   └── b
/// │       ├── alpha.txt
/// │       └── beta.txt
/// └── B
///     ├── a.txt
///     └── b
///         ├── alpha.txt
///         └── beta.txt
/// where alpha.txt, beta.txt and a.txt are duplicated.
///
/// Check that we got a duplicate object with two duplicate directories tempdir/A and tempdir/B.
fn duplicate_dirs_test() {
    // Create a temporary directory
    let tmp_dir = TempDir::new("add_directories_success_test").expect("Failed creating temp dir.");
    let tmp_dir_str = tmp_dir.path().to_owned().into_os_string();
    let tmp_dir_path = tmp_dir.path();

    // Create files and folders
    for topdir in ["A", "B"] {
        DirBuilder::new().create(tmp_dir_path.join(topdir));

        let a_file_path = tmp_dir_path.join(topdir).join("a.txt");
        write_file(&a_file_path, "test_text_a");

        let b_folder_path = tmp_dir_path.join(topdir).join("b");
        DirBuilder::new().create(&b_folder_path);

        write_file(&b_folder_path.join("alpha.txt"), "test_text_alpha");
        write_file(&b_folder_path.join("beta.txt"), "test_text_beta");

    }

    // Create args for DuDe
    let mut options = HashMap::new();
    options.insert("min_size".to_string(), 0);
    let paths = vec![tmp_dir_str.clone()];

    // Run DuDe
    let duplicates = duplicate_destroyer::get_duplicates(paths, options);

    // Check results
    let expected_duplicate = DuplicateObject::new(
        8235,
        HashSet::from([
            tmp_dir_path.join("A").into_os_string(),
            tmp_dir_path.join("B").into_os_string(),
        ])
    );

    assert_eq!(Ok(vec![expected_duplicate]), duplicates);

    // Prevent removing of tmp_dir until all tests are done
    tmp_dir.close();
}

#[test]
/// Create a directory structure with the schema
/// tempdir
/// ├── A
/// │   ├── a.txt
/// │   └── diff.txt
/// └── B
///     ├── a.txt
///     └── diff.txt
/// where a.txt is duplicated and diff.txt is not.
///
/// Check that we got a duplicate object with the files tempdir/A/a.txt and tempdir/B/a.txt .
fn duplicate_files_test() {
     // Create a temporary directory
    let tmp_dir = TempDir::new("add_directories_success_test").expect("Failed creating temp dir.");
    let tmp_dir_str = tmp_dir.path().to_owned().into_os_string();
    let tmp_dir_path = tmp_dir.path();

    // Create files and folders
    for topdir in ["A", "B"] {
        DirBuilder::new().create(tmp_dir_path.join(topdir));

        let a_file_path = tmp_dir_path.join(topdir).join("a.txt");
        write_file(&a_file_path, "test_text_a");

        let differing_file_path = tmp_dir_path.join(topdir).join("diff.txt");
        write_file(&differing_file_path, &["test_text_", topdir].join("") );

    }

    // Create args for DuDe
    let mut options = HashMap::new();
    options.insert("min_size".to_string(), 0);
    let paths = vec![tmp_dir_str.clone()];

    // Run DuDe
    let duplicates = duplicate_destroyer::get_duplicates(paths, options);

    // Check results
    let expected_duplicate = DuplicateObject::new(
        12,
        HashSet::from([
            tmp_dir_path.join("A/a.txt").into_os_string(),
            tmp_dir_path.join("B/a.txt").into_os_string(),
        ])
    );

    assert_eq!(Ok(vec![expected_duplicate]), duplicates);

    // Prevent removing of tmp_dir until all tests are done
    tmp_dir.close();
}
