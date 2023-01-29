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
use crate::DuplicateObject;

const CHCKSUM_LENGTH: usize = 1024;
// FIXME: this might differ per directory, get it dynamically
const DIR_SIZE: u64 = 4096;

/********************/
/*  NodeType Enum   */
/********************/

/// Enum for all the possible nodes in DirTree
#[derive(Debug)]
enum NodeType {
    File { path: OsString, size: u64, part_checksum: String, duplicates: HashSet<NodeId> },
    Dir { path: OsString, size: Option<u64>, duplicates: HashSet<NodeId> },
    Inaccessible { path: OsString, err: std::io::Error },
    Symlink { path: OsString },
}

/// Enum to flag child and parent nodes of nodes that are in duplicate list. To enable keeping only
/// the topmost duplicates.
enum IsContained {
    ChildOfDuplicate,
    ParentOfDuplicate,
    Duplicate,
    No,
}

impl NodeType {
    /// Get duplicates of the node
    fn duplicates(&self) -> Option<&HashSet<NodeId>> {
        match *self {
            Self::File { ref duplicates, .. } => Some(duplicates),
            Self::Dir { ref duplicates, .. } => Some(duplicates),
            Self::Symlink { .. } => None,
            Self::Inaccessible { .. } => None,
        }
    }

    /// Get path of node
    fn path(&self) -> &OsString {
        match self {
            Self::File { path, .. } => &path,
            Self::Dir { path, .. } => &path,
            Self::Symlink { path } => &path,
            Self::Inaccessible { path, .. } => &path,
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
    ///
    /// # Arguments
    /// * `num_threads` - number of threads to be created in duplicate table
    pub fn new(num_threads: usize) -> Self {
        let mut dir_tree = Tree::new();
        let root_node =
            NodeType::Dir { path: "ROOT_NODE".into(), size: None, duplicates: HashSet::new() };
        let root_id = dir_tree.insert(Node::new(RefCell::new(root_node)), AsRoot).unwrap();

        DirTree { dir_tree, root_id, duplicate_table: DuplicateTable::new(num_threads) }
    }

    /// Prints the dirtree structure.
    pub(crate) fn print<W: Write>(self, w: &mut W) {
        self.dir_tree.write_formatted(w).expect("Error writing dir_tree");
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
    /// or directories.
    pub(crate) fn add_directories<T: WithMetadata>(&mut self, dirs: Vec<T>) {
        for dir in dirs {
            log::info!("Adding directory {:?} to DirTree.", dir.filepath());
            // FIXME: Somehow solve this without cloning root_id? <05-11-22> //
            // FIXME: Also, maybe remove root_id from self? <05-11-22> //
            self._create_subtree(&dir, &self.root_id.clone());
            log::info!("Finished creating subtree");

            // Check if each dir we add is accessible to allow early killing by user
            // FIXME: Make this display only once per inaccessible node <06-11-22> //
            for child in self
                .dir_tree
                .children(&self.root_id)
                .expect("Could not access root node in dir_tree.")
            {
                if let NodeType::Inaccessible { path, err } = &*child.data().borrow() {
                    log::error!("Could not access directory {:?}: {}", path, err);
                }
            }
        }
    }

    /// Get the list of topmost duplicate groups.
    ///
    /// First we find duplicates for all nodes in DirTree. Then we create the list of duplicates -
    /// we go recusrively through the DirTree, whenever we find that a node has duplicates we add
    /// the duplicate group to the list and we don't search its children.
    pub(crate) fn get_duplicates(&mut self, min_size: u64) -> Vec<DuplicateObject> {
        log::info!("Getting duplicates.");
        // Get duplicates for all nodes
        self._find_duplicates();

        let mut duplicates: Vec<DuplicateObject> = vec![];
        let mut contained: Vec<DuplicateObject> = vec![];

        let root_ids = self._get_root_ids();
        for r_id in root_ids {
            self._recursively_get_duplicates(&r_id, min_size, &mut duplicates, &mut contained);
        }

        duplicates
    }

    /// Get the RefCell contained in node with `node_id`.
    fn _get_node_data(&self, node_id: &NodeId) -> &RefCell<NodeType> {
        let node_data =
            self.dir_tree.get(node_id).expect(&format!("Could not get node {node_id:?}")).data();
        node_data
    }

    /// Get path of node with `node_id`
    fn _get_node_path(&self, node_id: &NodeId) -> OsString {
        let node = &*self._get_node_data(node_id).borrow();
        node.path().to_owned()
    }

    /// Go through DirTree and add the largest duplicate groups to duplicate list
    ///
    /// Check whether node with `node_id` contains duplicates. If so, add them to duplicate vector.
    /// Otherwise recursively check all its children for duplicates as well.
    ///
    /// # Arguments
    /// * `node_id` - NodeId of the node that we want to search for duplicates
    /// * `duplicates` - Vector to add duplicate groups to
    // FIXME: Refactor this mess...
    fn _recursively_get_duplicates(
        &self,
        node_id: &NodeId,
        min_size: u64,
        duplicates: &mut Vec<DuplicateObject>,
        contained: &mut Vec<DuplicateObject>,
    ) {
        let node: &NodeType = &*self._get_node_data(node_id).borrow();
        match node {
            // Node has no duplicates, search children
            NodeType::Dir { duplicates: dir_duplicates, .. } if dir_duplicates.len() == 0 => {
                for child_id in self
                    .dir_tree
                    .children_ids(node_id)
                    .expect("Could not get children for id {node_id}")
                {
                    self._recursively_get_duplicates(child_id, min_size, duplicates, contained);
                }
            }
            // Node has duplicates, add it to dupl. list
            NodeType::Dir { duplicates: dir_duplicates, size, path }
                if dir_duplicates.len() > 0 =>
            {
                // Check that dir is not already present in some duplicate group
                if !DirTree::_duplicates_contain_path(duplicates, path)
                    && size.expect("Dir without size should not have duplicates.") > min_size
                {
                    self._add_duplicates_to_list(
                        path.clone(),
                        size.expect("Dir without size should not have duplicates."),
                        dir_duplicates.iter().map(|x| x.to_owned()).collect(),
                        duplicates,
                        contained,
                    );
                }
            }

            NodeType::File { duplicates: node_duplicates, size, path, .. }
                if node_duplicates.len() > 0 =>
            {
                if !DirTree::_duplicates_contain_path(duplicates, path) && *size > min_size {
                    self._add_duplicates_to_list(
                        path.clone(),
                        size.to_owned(),
                        node_duplicates.iter().map(|x| x.to_owned()).collect(),
                        duplicates,
                        contained,
                    );
                }
            }

            // For other node types do nothing
            _ => {}
        }
    }

    /// Add duplicate group to the list of duplicates
    ///
    /// We first check if one of the paths in the duplicate group is not already present in
    /// some other group. If not, we add the new group to the list of duplicates.
    ///
    /// # Arguments
    /// * `path` - path of the first member of the duplicate group
    /// * `size` - size ofeach member of the group
    /// * `data` - set of duplicates of `path`
    /// * `duplicates` - list of duplicate groups where the new group is added
    ///
    /// # Further explanation:
    /// Consider this arrangement of files:
    /// A - b - alpha.txt
    ///       - beta.txt
    ///
    /// B - b - alpha.txt
    ///       - beta.txt
    ///
    /// C - b - alpha.txt
    ///       - beta.txt
    ///   - delta.txt
    ///
    /// If we first search dir C for duplicates, we find group X = {A/b, B/b, C/b}. Then, searching
    /// dir A for duplicates, we find group Y = {A, B}. If we included both dirs in duplicates and
    /// then deleted e.g. dirs B/b and C/b from group 1 and dir A from group 2, we would
    /// accidentally delete all subdirs b in the process. We thus include only the top-most
    /// duplicate group.
    fn _add_duplicates_to_list(
        &self,
        path: OsString,
        size: u64,
        data: HashSet<NodeId>,
        duplicates: &mut Vec<DuplicateObject>,
        contained: &mut Vec<DuplicateObject>,
    ) {
        log::trace!("Adding {:?} to list of duplicates.", path);
        // If there is already a group with path in contained, do nothing
        if (contained.iter().map(|x| x.duplicates.iter()).flatten().any(|x| *x == path)) {
            return;
        }

        // For all paths in duplicate group look if it is not contained/contains path from
        // some already included duplicate group. If so, remove the group that is contained.
        let mut is_cont: Vec<DuplicateObject> = vec![];
        let mut cont: Vec<DuplicateObject> = vec![];

        let paths: Vec<_> = data.iter().map(|x| self._get_node_path(&x)).collect();
        //let paths: Vec<&OsString> = data.iter().map(|d| &d.path).collect();
        // FIXME: can I somehow do this without cloning duplicate objects?
        for path in paths.iter() {
            let (mut p_is_cont, mut p_cont): (Vec<_>, Vec<_>) = duplicates
                .iter()
                .map(|d_obj| (d_obj, d_obj.contained(&path)))
                .filter(|(d_obj, cont)| !cont.is_not_related())
                .map(|(d_obj, cont)| (d_obj.to_owned(), cont))
                .partition(|(d_obj, cont)| cont.is_contained());
            // Check some invariants
            assert!(
                p_is_cont.len() * p_cont.len() == 0,
                "One path cannot be contained and contain duplicate group."
            ); // From transitivity of containing
            assert!(p_is_cont.len() <= 1, "One path cannot be contained in more duplicate groups");
            for d_obj in p_cont.into_iter().map(|(d_obj, cont)| d_obj) {
                if !cont.contains(&d_obj) {
                    cont.push(d_obj)
                }
            }
            for d_obj in p_is_cont.into_iter().map(|(d_obj, cont)| d_obj) {
                if !is_cont.contains(&d_obj) {
                    is_cont.push(d_obj)
                }
            }
        }

        // Remove all duplicate objects that are contained in the paths in this group
        for dup_o in cont {
            duplicates.remove(
                duplicates
                    .iter()
                    .position(|x| *x == dup_o)
                    .expect(&format!("Duplicate object not found {dup_o:?}")),
            );
            contained.push(dup_o)
        }

        let mut paths: HashSet<OsString> = paths.into_iter().map(|x| x.to_owned()).collect();
        paths.insert(path.clone());
        if !(is_cont.len() > 0) {
            duplicates.push(DuplicateObject::new(size, paths));
        } else {
            contained.push(DuplicateObject::new(size, paths));
        }
    }

    /// Recursively go through all folders/files and create nodes with metadata for each
    ///
    /// # Arguments
    /// * `item` - a path to a file/directory to be included in the DirTree
    /// * `parent_node` - NodeId of the parent directory. Is id of root, if there is no parent dir.
    fn _create_subtree<T: WithMetadata>(&mut self, item: &T, parent_node: &NodeId) {
        let name = item.filepath();

        match item.metadata() {
            Ok(metadata) => {
                // item is dir
                if metadata.is_dir() {
                    // first check if we have permissions to read dir
                    log::info!("Reading dir: {name:?}");
                    match read_dir(&name) {
                        Ok(file_iter) => {
                            let node = NodeType::Dir {
                                path: name.clone(),
                                size: None,
                                duplicates: HashSet::new(),
                            };
                            let node_id = self._insert_node(node, parent_node);
                            // FIXME: This contains 1 unnecessary allocation, maybe redo? <05-11-22> //
                            // FIXME: This will probably crash on non-owned dirs. <05-11-22> //
                            for file in file_iter {
                                let file = file.expect("Could not reach a file.");
                                self._create_subtree(&file, &node_id);
                            }
                        }

                        // Dir not readable
                        Err(e) => {
                            log::info!("Could not access dir {:?}: {}", name, e);
                            let inac_node = NodeType::Inaccessible { path: name, err: e };
                            self._insert_node(inac_node, parent_node);
                        }
                    }

                // item is a file
                } else if metadata.is_file() {
                    // Symlinks get extra treatment
                    match get_partial_checksum::<CHCKSUM_LENGTH>(&name) {
                        Ok(checksum) => {
                            let node = NodeType::File {
                                path: name,
                                size: metadata.len(),
                                part_checksum: checksum.clone(),
                                duplicates: HashSet::new(),
                            };
                            let node_id = self._insert_node(node, parent_node);
                            self.duplicate_table.register_item(
                                checksum,
                                TableData { path: item.filepath(), node_id },
                            );
                        }
                        Err(e) => {
                            log::info!("Could not access dir {:?}: {}", name, e);
                            let inac_node = NodeType::Inaccessible { path: name, err: e };
                            self._insert_node(inac_node, parent_node);
                        }
                    };
                // item is not a file nor a dir.
                } else {
                    if metadata.is_symlink() {
                        let symlink_node = NodeType::Symlink { path: name };
                        self._insert_node(symlink_node, parent_node);

                    // File is just weird. (Probably named pipe though...)
                    // FIXME: Somehow get duplicates for named pipes as well?
                    } else {
                        log::warn!("File is not a dir nor file: {name:?}");
                        let e = std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Can not process named pipes.",
                        );
                        let inac_node = NodeType::Inaccessible { path: name, err: e };
                        self._insert_node(inac_node, parent_node);
                    }
                }
            } // Ok(metadata)

            // Item is inaccessible
            Err(e) => {
                log::info!("Could not access file {:?}: {}", name, e);
                let inac_node = NodeType::Inaccessible { path: name, err: e };
                self._insert_node(inac_node, parent_node);
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
    fn _insert_node(&mut self, node: NodeType, parent_node: &NodeId) -> NodeId {
        self.dir_tree
            .insert(Node::new(RefCell::new(node)), UnderNode(parent_node))
            .expect(&format!("Could not a insert node under this node: {parent_node:?}"))
    }

    /// Get `NodeId`s of the topmost directories in the DirTree
    ///
    /// (Returns NodeIds of nodes directly below root.)
    fn _get_root_ids(&self) -> Vec<NodeId> {
        let root_ids: Vec<NodeId> = self
            .dir_tree
            .children_ids(&self.root_id)
            .expect("DirTree has to have some subtrees by now.")
            .map(|x| x.to_owned())
            .collect();
        root_ids
    }

    /// Gets the duplicates for each node in DirTree.
    ///
    /// Traverses the duplicate tree post-order and gets duplicates from duplicate table for each
    /// FileNode. For each DirNode
    fn _find_duplicates(&mut self) {
        // FIXME: This is kinda hackish //
        // Get all root dirs processed
        log::info!("Finding duplicates.");
        let root_ids: Vec<_> = self._get_root_ids();

        // Go through all root dirs and get duplicates for each node
        for root_id in root_ids.iter() {
            for id in self
                .dir_tree
                .traverse_post_order_ids(root_id)
                .expect(&format!("Could not traverse tree for {root_id:?}"))
            {
                let node_data = self._get_node_data(&id);
                match *node_data.borrow_mut() {
                    NodeType::File { ref mut duplicates, ref part_checksum, ref path, .. } => {
                        self._add_duplicates_to_file_entry(
                            id,
                            duplicates,
                            part_checksum,
                            path.to_owned(),
                        );
                    }
                    NodeType::Dir { ref mut duplicates, ref path, .. } => {
                        self._get_possible_dupl_for_dirs(&id, duplicates, path);
                    }
                    _ => {}
                }
            }
        }

        // Go through root_dirs again filtering out false dir duplicates and setting dir size
        for root_id in root_ids {
            for id in self
                .dir_tree
                .traverse_post_order_ids(&root_id)
                .expect(&format!("Could not traverse tree for {root_id:?}"))
            {
                let node_data = self._get_node_data(&id);
                if let NodeType::Dir { ref mut duplicates, ref mut size, ref path } =
                    *node_data.borrow_mut()
                {
                    self._filter_dir_duplicates(&id, duplicates);
                    self._set_dir_size(&id, size, path);
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
        &self,
        node_id: NodeId,
        node_duplicates: &mut HashSet<NodeId>,
        part_checksum: &str,
        path: OsString,
    ) {
        // FIXME: Do this without cloning entry path? //
        let data = TableData { path, node_id };
        let rec_duplicates = self.duplicate_table.get_duplicates(part_checksum, &data);

        match rec_duplicates {
            Err(e) => panic!("Getting duplicates failed: {e}"),
            Ok(dupl) => {
                *node_duplicates = dupl.into_iter().map(|table_data| table_data.node_id).collect();
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
    /// * `node_id` - NodeId of the dir node whose duplicates we want
    /// * `dir_node` - data of the node whose duplicates we want
    fn _get_possible_dupl_for_dirs(
        &self,
        node_id: &NodeId,
        node_duplicates: &mut HashSet<NodeId>,
        path: &OsString,
    ) {
        // FIXME: Handle empty dirs - If a dir contains empty dirs we might consider it duplicate
        // to another dir with the same empty dirs. Could be solved similarly to symlinks. //
        // FIXME: Handle symlinks correctly.
        log::info!("Getting possible duplicates for: {:?}", path);
        let mut children =
            self.dir_tree.children(node_id).expect("Could not get dirtree children.");
        let mut result: HashSet<NodeId>;
        // Get first set of duplicates
        if let Some(child) = children.next() {
            let data = child.data().borrow();
            result = match data.duplicates() {
                None => return, // child node is inaccessible (or symlink), dir not duplicate
                Some(hs) if hs.len() == 0 => return, // child node has no duplicates, so dir not
                // duplicate
                Some(hs) => {
                    hs.iter().map(|x| self._get_parent_table_data(x)).filter_map(|x| x).collect()
                }
            };
        } else {
            // No child nodes, nothing to do...
            return;
        }

        // For each child get intersection of duplicates
        for child in children {
            let data = child.data().borrow();
            let parent_duplicates: HashSet<NodeId> = match data.duplicates() {
                None => return, // child node is inaccessible (or symlink), dir not duplicate
                Some(hs) if hs.len() == 0 => return, // child node has no duplicates, dir not dupl.
                Some(hs) => {
                    hs.iter().map(|x| self._get_parent_table_data(x)).filter_map(|x| x).collect()
                }
            };
            result.retain(|x| parent_duplicates.contains(x))
        }

        // If we have e.g. a dir that has only a file and its copy, we would get that the dir
        // itself is its duplicate. Remove such case.
        result.retain(|x| x != node_id);

        *node_duplicates = result;
    }

    /// Get TableData for a parent dir
    ///
    /// # Arguments
    /// * `data` - Data of a node whose parent data should be returned
    fn _get_parent_table_data(&self, data: &NodeId) -> Option<NodeId> {
        let parent_id = self.dir_tree.get(data).unwrap().parent();

        // Return None if we are at topmost node.
        match parent_id {
            Some(id) if *id != self.root_id => Some(id.to_owned()),
            _ => None,
        }
    }

    pub(crate) fn finalise(&mut self) {
        self.duplicate_table.finalise();
    }

    /// Set the size of DirNode
    ///
    /// Goes through all the children of DirNode and calculates its size.
    ///
    /// # Arguments
    /// `node_id` - NodeId of the node whose size is being set
    /// `node` - node whose size is being set
    // FIXME: This is not accurate (maybe due to the constant DIR_SIZE?)
    fn _set_dir_size(&self, node_id: &NodeId, size: &mut Option<u64>, path: &OsString) {
        log::info!("Setting dir size for: {:?}", path);
        let children = self.dir_tree.children(node_id).expect("Could not get dirtree children.");
        let mut result = 0u64;

        for child in children {
            match &*child.data().borrow() {
                NodeType::File { size, .. } => {
                    result += size;
                }
                // If size of subdir is known, add it. Otherwise, leave set to None
                NodeType::Dir { size: Some(size), .. } => {
                    result += size;
                }
                // Size of subdir is unknown, leave set to None
                NodeType::Dir { size: None, .. } => {
                    return;
                }
                // Size is unknown, leave set to None
                NodeType::Inaccessible { .. } => {
                    return;
                }
                // FIXME: Count symlink size
                NodeType::Symlink { .. } => {}
            }
        }

        // count the size of the directory listing as well
        *size = Some(result + DIR_SIZE);
    }

    /// Filter DirNode duplicates so that only real duplicates remain
    ///
    /// If a dir A contains all files in dir B, but dir B contains files not in dir A, we would
    /// get that B is contained in duplicates of A even though they are not duplicates.
    ///
    /// This function goes through all duplicates of a node and removes the duplicates that don't
    /// have the node in their duplicates as well.
    ///
    /// # Arguments
    /// * `node_id` - NodeId of the node whose duplicates should be filtered
    /// * `node` - node whose duplicates should be filtered
    /// `node_id` should be id of `node`.
    fn _filter_dir_duplicates(&self, node_id: &NodeId, node_duplicates: &mut HashSet<NodeId>) {
        log::info!("Filtering duplicates for: {:?}", self._get_node_path(node_id));
        node_duplicates.retain(|x| self._is_duplication_mutual(node_id, &x));
    }

    /// Check whether node with `first_id` is in duplicates of node with `other_id`
    ///
    /// # Arguments
    /// * `first_id` - NodeId of the node that should be in `other_id`s duplicates
    /// * `other_id` - NodeId of the node that should have `first_id` as a duplicate
    fn _is_duplication_mutual(&self, first_id: &NodeId, other_id: &NodeId) -> bool {
        let other_node = self._get_node_data(other_id).borrow();
        if let Some(hs) = other_node.duplicates() {
            hs.iter().map(|x| x).any(|x| x == first_id)
        } else {
            false
        }
    }

    fn _duplicates_contain_path(duplicates: &Vec<DuplicateObject>, path: &OsString) -> bool {
        duplicates.iter().map(|x| x.duplicates.iter()).flatten().any(|x| *x == *path)
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
        let dt = DirTree::new(0);
        let mut out = String::new();
        dt.print(&mut out);
        let expected_tree =
            "RefCell { value: Dir { path: \"ROOT_NODE\", size: None, duplicates: {} } }\n";
        assert_eq!(expected_tree, out);
    }
}
