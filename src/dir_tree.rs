//! Directory structure used for duplicate discovery
//!
//! This module provides a directory tree with metadata used to find duplicate files and folders.
//! The basis of the module is the DirTree structure that contains tree with nodes representing
//! files or directories.
//!
//! When the tree gets populated we also calculate hashes of the first CHCKSUM_LENGTH bytes of
//! files and register them in the duplicate_table, which helps us find duplicates.
//!
//! # Example of use inside the crate
//! ```compile_fail
//! // Note that this uses crate-only public functions, so it will not compile outside of crate
//! use duplicate_destroyer::dir_tree::DirTree;
//!
//! let mut dt = DirTree::new();
//! dt.add_directories("/path/to/file");
//! let mut s = String::new();
//! dt.print(s)
//! ```

use core::fmt::Write;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{DirEntry, read_dir, Metadata};
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;

use id_tree::{InsertBehavior::*, Node, NodeId, Tree};
use users::{get_current_gid, get_current_uid};

use crate::checksum::get_partial_checksum;
use crate::duplicate_table::register_checksum;

const CHCKSUM_LENGTH: usize = 100;

/// Struct with metadata for files
#[derive(Debug)]
struct FileNode {
    pub path: OsString,
    pub size: u64,
    pub checksum: String,
    pub duplicates: HashSet<Rc<NodeType>>,
}

/// Struct with metadata for directories
#[derive(Debug)]
struct DirNode {
    pub path: OsString,
    pub size: Option<u64>,
    pub duplicates: HashSet<Rc<NodeType>>,
}

/// Struct for inaccessible files
#[derive(Debug)]
struct InaccessibleNode {
    pub path: OsString,
    pub err: std::io::Error,
}

/// Struct for nodes with empty files
#[derive(Debug)]
struct ZeroLenNode {
    pub path: OsString,
}

/// Struct for symlink nodes
#[derive(Debug)]
struct SymlinkNode {
    pub path: OsString,
}

/// Enum for all the possible nodes in DirTree
#[derive(Debug)]
enum NodeType {
    File(FileNode),
    Dir(DirNode),
    Inaccessible(InaccessibleNode),
    ZeroLen(ZeroLenNode),
    Symlink(SymlinkNode),
}

/// Describes the directory structure
#[derive(Debug)]
pub struct DirTree {
    dir_tree: Tree<NodeType>,
    root_id: NodeId,
}

impl DirTree {
    /// Create new empty DirTree
    pub fn new() -> Self {
        let mut tree: Tree<NodeType> = Tree::new();
        let root_node = NodeType::Dir(DirNode {
            path: "ROOT_NODE".into(),
            size: None,
            duplicates: HashSet::new(),
        });
        let root_id = tree.insert(Node::new(root_node), AsRoot).unwrap();
        DirTree {
            dir_tree: tree,
            root_id,
        }
    }

    /// Add directories (and files) to the DirTree
    ///
    /// Takes a vector of paths and for each path it recursively goes through all subdirectories
    /// and gathers file metadata and populates the duplicate table.
    ///
    /// If a child file can't be read due to permissions, the function prints warning and storres
    /// it as InaccessibleNode in DirTree.
    ///
    /// # Arguments
    /// `paths` - Vector of paths where the duplicates should be searched. Can be paths of files
    /// and directories.
    pub(crate) fn add_directories<T: WithMetadata>(&mut self, dirs: Vec<T>) {
        for dir in dirs {
            // FIXME: Somehow solve this without cloning root_id? <05-11-22> //
            // FIXME: Also, maybe remove root_id from self? <05-11-22> //
            self.create_subtree(&dir, &self.root_id.clone());
            // FIXME: Make this display only once per inaccessible node <06-11-22> //
            for child in self
                .dir_tree
                .children(&self.root_id)
                .expect("Could not access root node in dir_tree.")
            {
                if let NodeType::Inaccessible(InaccessibleNode { path, err }) = child.data() {
                    log::error!("Could not access directory {:?}: {}", path, err);
                }
            }
        }
    }

    /// Recursively go through all folders/files and create nodes with metadata for each
    ///
    /// # Arguments
    /// * `item` - a path to a file/directory to be included in the DirTree
    /// * `parent_node` - NodeId of the parent directory. Is id of root, if there is no parent dir.
    fn create_subtree<T: WithMetadata>(&mut self, item: &T, parent_node: &NodeId) {
        let name = item.filepath();

        match item.metadata() {
            Ok(metadata) => {
                // item is dir
                if metadata.is_dir() {
                    // first check if we have permissions to read dir
                    match read_dir(&name) {
                        Ok(file_iter) => {
                            let node = NodeType::Dir(DirNode {
                                path: name.clone(),
                                size: None,
                                duplicates: HashSet::new(),
                            });
                            let node_id = self.insert_node(node, parent_node);
                            // FIXME: This contains 1 unnecessary allocation, maybe redo? <05-11-22> //
                            // FIXME: This will probably crash on non-owned dirs. <05-11-22> //
                            for file in read_dir(&item.filepath()).expect("Could not read dir.") {
                                let file = file.expect("Could not reach a file.");
                                self.create_subtree(&file, &node_id);
                            }
                        }

                        // Dir not readable
                        Err(e) => {
                            log::info!("Could not access dir {:?}: {}", name, e);
                            let inac_node =
                                NodeType::Inaccessible(InaccessibleNode { path: name, err: e });
                            self.insert_node(inac_node, parent_node);
                        }
                    }

                // item is a file
                } else {
                    // Can't get checksum of empty files.
                    if metadata.len() == 0 {
                        let zero_len_node = NodeType::ZeroLen(ZeroLenNode { path: name });
                        self.insert_node(zero_len_node, parent_node);

                    // Symlinks get extra treatment
                    } else if metadata.is_symlink() {
                        let symlink_node = NodeType::Symlink(SymlinkNode { path: name });
                        self.insert_node(symlink_node, parent_node);

                    // Item is regular nonempty file
                    } else {
                        match get_partial_checksum::<CHCKSUM_LENGTH>(&name) {
                            Ok(checksum) => {
                                let node = NodeType::File(FileNode {
                                    path: name,
                                    size: metadata.len(),
                                    checksum: checksum.clone(),
                                    duplicates: HashSet::new(),
                                });
                                let node_id = self.insert_node(node, parent_node);
                                register_checksum(checksum, item.filepath(), node_id);
                            }
                            Err(e) => {
                                log::info!("Could not access dir {:?}: {}", name, e);
                                let inac_node =
                                    NodeType::Inaccessible(InaccessibleNode { path: name, err: e });
                                self.insert_node(inac_node, parent_node);
                            }
                        };
                    }
                }
            }

            // Item is inaccessible
            Err(e) => {
                log::info!("Could not access file {:?}: {}", name, e);
                let inac_node = NodeType::Inaccessible(InaccessibleNode { path: name, err: e });
                self.insert_node(inac_node, parent_node);
            }
        }
    }

    /// Wrapper over tree insert method. Panics, if insertion throws error.
    ///
    /// # Arguments
    /// * `node` - Contents of the node to be inserted
    /// * `parent_node` - NodeId of the node the `node` should be inserted under
    ///
    /// # Panics
    /// Panics if the insertion fails. We don't remove nodes from the tree, so if that happens
    /// something is really broken.
    fn insert_node(&mut self, node: NodeType, parent_node: &NodeId) -> NodeId {
        self.dir_tree
            .insert(Node::new(node), UnderNode(parent_node))
            .expect("Could not a insert node under this node: {parent_node:?}")
    }

    /// Prints the dirtree structure.
    pub(crate) fn print<W: Write>(&self, w: &mut W) {
        self.dir_tree
            .write_formatted(w)
            .expect("Error writing dir_tree");
    }
}

/// Trait used to unify behaviour of OsString and DirEntry for create_subtree
pub(crate) trait WithMetadata {
    fn metadata(&self) -> std::io::Result<Metadata>;
    fn filepath(&self) -> OsString;
}

impl WithMetadata for OsString {
    fn metadata(&self) -> std::io::Result<Metadata> {
        std::fs::metadata(self)
    }

    fn filepath(&self) -> OsString {
        self.clone()
    }
}

impl WithMetadata for DirEntry {
    fn metadata(&self) -> std::io::Result<Metadata> {
        self.metadata()
    }

    fn filepath(&self) -> OsString {
        self.path().into_os_string()
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;
    // TODO: Add tests for WithMetadata <07-11-22> //
    // TODO: Add tests for logging
    // TODO: Add tests for failing file access

    #[test]
    fn dirtree_new_test() {
        let dt = DirTree::new();
        let mut out = String::new();
        dt.print(&mut out);
        let expected_tree = "Dir(DirNode { path: \"ROOT_NODE\", size: None, duplicates: {} })\n";
        assert_eq!(expected_tree, out);
    }
}
