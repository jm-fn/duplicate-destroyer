# Duplicate Destroyer
Command line tool that finds duplicate directories and provides their basic handling.

## The Pitch

Have you ever backed up a backup folder of a backup folder? Have you then tried to deduplicate the tangled mess with conventional deduplicator only to find that you have to check 20 431 files manually? Then the DuDe is for you! DuDe finds the topmost duplicate folders in your filesystem and allows you to effortlessly get rid of all of your duplicates once and for all (or at least until the next backup...).

...at least that's the sales pitch. It's not there yet. 

(Also this is a small project intended as a learning experience with Rust.)

# Installation

TBD. For now the only way is cloning the repo and building from source. I have so far only tested the code with Rust v1.59 on Fedora 35. 


# Basic Usage 

Scan a directory for duplicates
```bash
duplicate_destroyer -p path/to/some/dir -p path/to/another/dir
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
will delete "path/to/dir/some_dir/A" in our example. (This is not implemented yet.)
