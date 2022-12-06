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
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{read_dir, DirEntry, Metadata};
use std::path::Path;

use id_tree::{InsertBehavior::*, Node, NodeId, Tree};

use crate::checksum::get_partial_checksum;
use crate::duplicate_table::DuplicateTable;

const CHCKSUM_LENGTH: usize = 256;

/********************/
/*  NodeType Enum   */
/********************/

/// Struct with metadata for files
#[derive(Debug)]
struct FileNode {
    pub path: OsString,
    pub size: u64,
    pub part_checksum: String,
    pub duplicates: HashSet<TableData>,
}

/// Struct with metadata for directories
#[derive(Debug)]
struct DirNode {
    pub path: OsString,
    pub size: Option<u64>,
    pub duplicates: HashSet<TableData>,
}

/// Struct for inaccessible files
#[derive(Debug)]
struct InaccessibleNode {
    pub path: OsString,
    pub err: std::io::Error,
}

// FIXME: Add duplicates to symlinks as well //
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
    Symlink(SymlinkNode),
}

impl NodeType {
    /// Get duplicates of the node
    fn duplicates(&self) -> Option<&HashSet<TableData>> {
        match self {
            Self::File(val) => Some(&val.duplicates),
            Self::Dir(val) => Some(&val.duplicates),
            Self::Symlink(_) => None,
            Self::Inaccessible(_) => None,
        }
    }
}

/*************************/
/*   DirTree Structure   */
/*************************/

/// Describes the directory structure
#[derive(Debug)]
pub(crate) struct DirTree {
    dir_tree: Tree<RefCell<NodeType>>,
    root_id: NodeId,
    duplicate_table: DuplicateTable,
}

impl DirTree {
    /// Create new empty DirTree
    pub fn new() -> Self {
        let mut tree = Tree::new();
        let root_node = NodeType::Dir(DirNode {
            path: "ROOT_NODE".into(),
            size: None,
            duplicates: HashSet::new(),
        });
        let root_id = tree
            .insert(Node::new(RefCell::new(root_node)), AsRoot)
            .unwrap();
        DirTree {
            dir_tree: tree,
            root_id,
            duplicate_table: DuplicateTable::new(),
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
                if let NodeType::Inaccessible(InaccessibleNode { path, err }) =
                    &*child.data().borrow()
                {
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
                            for file in file_iter {
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
                    // Symlinks get extra treatment
                    if metadata.is_symlink() {
                        let symlink_node = NodeType::Symlink(SymlinkNode { path: name });
                        self.insert_node(symlink_node, parent_node);

                    // Item is regular nonempty file
                    } else {
                        match get_partial_checksum::<CHCKSUM_LENGTH>(&name) {
                            Ok(checksum) => {
                                let node = NodeType::File(FileNode {
                                    path: name,
                                    size: metadata.len(),
                                    part_checksum: checksum.clone(),
                                    duplicates: HashSet::new(),
                                });
                                let node_id = self.insert_node(node, parent_node);
                                self.duplicate_table.register_item(
                                    checksum,
                                    TableData {
                                        path: item.filepath(),
                                        node_id,
                                    },
                                );
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
            .insert(Node::new(RefCell::new(node)), UnderNode(parent_node))
            .expect("Could not a insert node under this node: {parent_node:?}")
    }

    /// Prints the dirtree structure.
    pub(crate) fn print<W: Write>(self, w: &mut W) {
        self.dir_tree
            .write_formatted(w)
            .expect("Error writing dir_tree");
    }

    /// Gets the duplicates for each node in DirTree.
    ///
    /// Traverses the duplicate tree post-order and gets duplicates from duplicate table for each
    /// FileNode. For each DirNode
    // FIXME: Make private //
    pub(crate)fn _find_duplicates(&mut self) {
        // FIXME: This is kinda hackish //
        // Get all root dirs processed
        let root_ids: Vec<&NodeId> = self
            .dir_tree
            .children_ids(&self.root_id)
            .expect("DirTree has to have some subtrees by now.")
            .collect();

        // Go through all root dirs
        for root_id in root_ids {
            for id in self.dir_tree
                .traverse_post_order_ids(root_id)
                .expect("Could not traverse tree for {root_id}")
            {
                let node = self.dir_tree
                    .get(&id)
                    .expect("Could not get a node with id {id:?}.");
                match &mut *node.data().borrow_mut() {
                    NodeType::File(entry) => {
                        DirTree::_add_duplicates_to_file_entry(id, entry, &self.duplicate_table);
                    }
                    NodeType::Dir(entry) => {
                        self._get_possible_dupl_for_dirs(id, entry);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Gets duplicates of a file from the duplicate table and writes them to the data of the
    /// corresponding node in DirTree.
    ///
    /// # Arguments
    /// * `node_id` - node id of the file node in the DirTree
    /// * `entry` - the node data where the duplicates should be added
    /// * `table` - duplicate table where the duplicates are searched
    /// `entry` corresponds to the data of the node with `node_id`
    ///
    /// # Panics
    /// Panics when we can't get duplicates from the DuplicateTable.
    fn _add_duplicates_to_file_entry(
        node_id: NodeId,
        entry: &mut FileNode,
        table: &DuplicateTable,
    ) {
        // FIXME: Do this without cloning entry path? //
        let data = TableData {
            path: entry.path.clone(),
            node_id,
        };
        let duplicates = table.get_duplicates(&entry.part_checksum, &data);

        match duplicates {
            Err(e) => panic!("Getting duplicates failed: {e}"),
            Ok(dupl) => {
                entry.duplicates = dupl;
            }
        }
    }

    /// Find all potential duplicate directories for a dir node
    ///
    /// Goes through all children of a `dir_node`, finds parents of their duplicates and gets the
    /// subset of the parents that are parents for all of the files (dirs) under the `dir_node`.
    /// These make up the set of all possible duplicate dirs for the `dir_node`.
    ///
    /// Note that the possible dirs found this way may not really be duplicate, as they can contain
    /// additional files that `dir_node` does not. This is solved by
    ///
    /// # Arguments
    fn _get_possible_dupl_for_dirs(&self, node_id: NodeId, dir_node: &mut DirNode) {
        // FIXME: Handle empty dirs - If a dir contains empty dirs we might consider it duplicate
        // to another dir with the same empty dirs. Could be solved similarly to symlinks. //
        // FIXME: Handle symlinks correctly.
        let mut children = self
            .dir_tree
            .children(&node_id)
            .expect("Could not get dirtree children.");
        let mut result = HashSet::new();
        // Get first set of duplicates
        if let Some(child) = children.next() {
            let data = child.data().borrow();
            result = match data.duplicates() {
                None => return, // child node is inaccessible (or symlink), dir not duplicate
                Some(hs) if hs.len() == 0 => return, // child node has no duplicates, so dir not
                // duplicate
                Some(hs) => hs.iter().map(|x| self._get_parent_table_data(x)).collect(),
            };
        } else {
            // No child nodes, nothing to do...
            return;
        }

        // For each child get intersection of duplicates
        for child in children {
            let data = child.data().borrow();
            let parent_duplicates: HashSet<TableData> = match data.duplicates() {
                None => return, // child node is inaccessible (or symlink), dir not duplicate
                Some(hs) if hs.len() == 0 => return, // child node has no duplicates, dir not dupl.
                Some(hs) => hs.iter().map(|x| self._get_parent_table_data(x)).collect(),
            };
            result.retain(|x| parent_duplicates.contains(x))
        }

        dir_node.duplicates = result;
    }

    /// Get TableData for a parent dir
    ///
    /// # Arguments
    /// * `data` - Data of a node whose parent data should be returned
    fn _get_parent_table_data(&self, data: &TableData) -> TableData {
        let parent_path = Path::new(&data.path)
            .parent()
            .expect("Could not get parent path of {data.path}")
            .as_os_str()
            .to_owned();
        let parent_id = self
            .dir_tree
            .get(&data.node_id)
            .unwrap()
            .parent()
            .expect("Could not get parent id of {data.node_id}.")
            .to_owned();
        TableData {
            path: parent_path,
            node_id: parent_id,
        }
    }
}

/**************************/
/*   WithMetadata Trait   */
/**************************/

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

/***************************/
/*   TableData Structure   */
/***************************/

/// Struct with data identifying node corresponding to file. Used as interface for DuplicateTable
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub(crate) struct TableData {
    path: OsString,
    node_id: NodeId,
}

impl TableData {
    /// Get path to file
    pub(crate) fn path(&self) -> &OsString {
        &self.path
    }
}

/******************/
/*   Unit Tests   */
/******************/

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
