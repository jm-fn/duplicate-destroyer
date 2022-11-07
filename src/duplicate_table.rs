use id_tree::NodeId;
use std::ffi::OsString;

/// Placeholder for register checksum.
pub fn register_checksum(checksum: String, name: OsString, node_id: NodeId) {
    println!("Register checksum function for {:?}", name);
}
