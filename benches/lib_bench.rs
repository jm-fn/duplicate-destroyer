use criterion::{criterion_group, criterion_main, Criterion};
use duplicate_destroyer;
use std::fs::{DirBuilder, File};
use std::io::{self, Write};
use std::path::Path;
use tempdir::TempDir;

fn write_file(path: &Path, contents: &str) -> io::Result<()>{
    let mut file = File::create(path).expect("Could not create a file.");
    writeln!(file, "{}", contents)?;
    Ok(())
}

fn bench_compare_two_identical_dirs(c: &mut Criterion) -> io::Result<()>{
    let tmp_dir = TempDir::new("add_directories_success_test").expect("Failed creating temp dir.");
    let tmp_dir_str = tmp_dir.path().to_owned().into_os_string();
    let tmp_dir_path = tmp_dir.path();

    let greek_alphabet = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
        "chi", "psi", "omega",
    ];

    // Create files and folders
    for topdir in ["A", "B"] {
        DirBuilder::new().create(tmp_dir_path.join(topdir))?;

        let a_file_path = tmp_dir_path.join(topdir).join("a.txt");
        write_file(&a_file_path, "test_text_a")?;

        let b_folder_path = tmp_dir_path.join(topdir).join("b");
        DirBuilder::new().create(&b_folder_path)?;

        for letter in greek_alphabet {
            write_file(
                &b_folder_path.join(format!("{letter}.txt")),
                &format!("test_text_{letter}"),
            )?;
        }
    }

    // Create args for DuDe
    let mut options: duplicate_destroyer::Config = Default::default();
    options.set_minimum_size(0);
    let paths = vec![tmp_dir_str.clone()];

    c.bench_function("iter", move |b| {
        b.iter(|| duplicate_destroyer::get_duplicates(paths.clone(), &options));
    });

    Ok(())
}

criterion_group!(benches, bench_compare_two_identical_dirs);
criterion_main!(benches);
