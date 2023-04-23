
# Duplicate Destroyer
Command line tool that finds duplicate directories and provides their basic handling.

[![Tests](https://github.com/jm-fn/duplicate-destroyer/actions/workflows/tests.yml/badge.svg)](https://github.com/jm-fn/duplicate-destroyer/actions/workflows/tests.yml)
[![crates.io](https://img.shields.io/crates/v/duplicate_destroyer.svg)](https://crates.io/crates/duplicate_destroyer)
[![docs.rs](https://img.shields.io/docsrs/duplicate_destroyer)](https://docs.rs/duplicate_destroyer/latest/duplicate_destroyer)

## The Pitch

Have you ever backed up a backup folder of a backup folder? Have you then tried to deduplicate the tangled mess with conventional deduplicator only to find that you have to check 20 431 files manually? Then the DuDe is for you! DuDe finds the topmost duplicate folders in your filesystem and allows you to effortlessly get rid of all of your duplicates once and for all (or at least until the next backup...).

(Also this is a small project intended as a learning experience with Rust.)

# Installation

## From Source

On Linux with Rust 1.64 or higher install by running:

```
cargo install --features cli duplicate_destroyer
```
After the installation is finished, there will be `dude` binary available.

I have so far tested the installation on Fedora 35+ and on Raspberry Pi OS Bullseye.

### On Ubuntu 22.04 LTS
There may be a missing build dependency - `cc`. To install the DuDe first run
```
apt install build-essential
```
and then build from source
```
cargo install --features cli duplicate_destroyer
```

# Basic Usage

> **Warning:**
> The crate is still pretty new and there are some big changes to the API to be expected.


Scan a directory for duplicates
```bash
dude --path path/to/some/dir --path path/to/another/dir
```
Once the directory is scanned DuDe will print the duplicate groups found. E.g.:
```bash
Group 1/2
--------------------------------
0. "path/to/some/dir/some_dir/A"
1. "path/to/some/dir/other_dir/B"
--------------------------------
Size: 8kB
-----------
Select action and paths. (Or press Ctrl-C to exit program.)
[O]pen, Open [F]older, [D]elete, ReplaceWith[H]ardlink, ReplaceWith[S]oftlink, [N]othing
```
To act on the items found type the letter of action and file numbers. E.g.
```bash
O 0 1
```
will open both files.
```
D 0
```
will (upon confirmation) delete "path/to/dir/some_dir/A" in our example.

### Parallelism
To configure the number of threads used in calculating checksums use the `--jobs` flag:
```
dude --path path/to/some/dir --jobs 3
```
When using the DuDe with a modern CPU and an external HDD it is usually better to use only one thread (as is the default now), since the program then becomes IO-bound and the parallel access to multiple files from the HDD can reduce the read speed.

### Minimum-size
The minimum size of the duplicates returned can be specified with the `--minimum-size` argument. Note however, that this will not significantly reduce the computation time, since the DuDe still gets the checksum of all the files that might have duplicates. This is done because even large directories might differ in some small files and by disregarding the small files completely we would run the risk of losing some small but important data.

### CLI options
```
Usage: dude [OPTIONS]

Options:
  -p, --path <PATH>                  Add path to be scanned
  -m, --minimum-size <MINIMUM_SIZE>  Minimul size of duplicates considered (can have a metric prefix) [100]
  -j, --jobs <JOBS>                  Number of jobs that run simultaneously
      --json-file <FILE>             Output the list of duplicates to a file in json format
      --no-interactive               Disable interactive duplicate handling
  -h, --help                         Print help
  -V, --version                      Print version
```

# The Library
If you do not like the user interface, you can write your own! The DuDe exposes a library with the core functionality. See the documentation [here](https://docs.rs/duplicate_destroyer/latest/duplicate_destroyer/).
