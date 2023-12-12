//! Checksum calculation module
use std::ffi::OsString;
use std::fs::File;
use std::io::{prelude::Read, BufReader, Result};

use digest::Digest;

#[derive(Copy, Clone)]
/// Hash Algorithm types supported
pub enum HashAlgorithm {
    Blake2,
    SHA3_256,
    SHA3_512,
}

/// Get function that calculates checksum of whole file
///
/// # Arguments
/// * `ha` - hash algorithm that is used to calculate the checksum
pub(crate) fn get_checksum_fn(ha: &HashAlgorithm) -> fn(&OsString) -> Result<String> {
    match ha {
        HashAlgorithm::Blake2 => get_checksum::<blake2::Blake2b512>,
        HashAlgorithm::SHA3_256 => get_checksum::<sha3::Sha3_256>,
        HashAlgorithm::SHA3_512 => get_checksum::<sha3::Sha3_512>,
    }
}

/// Calculate checksum for a whole file
///
/// # Arguments
/// * `path` - path to the file to be checksummed
/// * `H` - hasher structure that is used for checksum calculation
fn get_checksum<H>(path: &OsString) -> Result<String>
where
    H: Digest,
    digest::Output<H>: std::fmt::LowerHex,
{
    log::trace!("Getting checksum for {:?}", path);
    let mut hasher = H::new();
    let mut buffer = [0u8; 1024];

    let mut buf_reader = BufReader::new(File::open(path)?);

    loop {
        let count = buf_reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    let result = format!("{:x}", hasher.finalize());
    Ok(result)
}

/// Get function that calculates checksum of first LEN bytes of file
///
/// # Arguments
/// * `ha` - hash algorithm that is used to calculate the checksum
pub(crate) fn get_partial_checksum_fn<const LEN: usize>(
    ha: &HashAlgorithm,
) -> fn(&OsString) -> Result<String> {
    match *ha {
        HashAlgorithm::Blake2 => get_partial_checksum::<LEN, blake2::Blake2b512>,
        HashAlgorithm::SHA3_256 => get_partial_checksum::<LEN, sha3::Sha3_256>,
        HashAlgorithm::SHA3_512 => get_partial_checksum::<LEN, sha3::Sha3_512>,
    }
}

/// Calculate checksum of first LEN bytes of a file
///
/// Returns checksum of first LEN bytes of file or io::Error.
///
/// For H == blake2::Blake2b512 this is equivalent to `head -c${LEN} path | b2sum`.
///
/// # Arguments
/// * `LEN` - constant, max number of bytes of file used for checksum calculation.
///           If file size is smaller than LEN, get_partial_checksum uses the whole file.
/// * `path` - path to file to be checksummed
/// * `H` - hasher structure that is used for checksum calculation
fn get_partial_checksum<const LEN: usize, H>(path: &OsString) -> Result<String>
where
    H: Digest,
    digest::Output<H>: std::fmt::LowerHex,
{
    let mut hasher = H::new();
    let mut buffer = [0u8; LEN];

    let mut input = File::open(path)?;
    let count = input.read(&mut buffer)?;
    hasher.update(&buffer[..count]);
    let result = format!("{:x}", hasher.finalize());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::prelude::*;
    use tempdir::TempDir;

    #[test]
    fn blake2_partial_test() -> Result<()> {
        // Prepare test file
        let tmp_dir = TempDir::new("duplicate_destroyer_test_dir")?;
        let file_path = tmp_dir.path().join("test_file.txt");
        let mut tmp_file = File::create(file_path.clone())?;
        writeln!(tmp_file, "This is a test string.")?;
        drop(tmp_file);

        // Check get_partial_checksum
        let checksum = get_partial_checksum::<100, blake2::Blake2b512>(&OsString::from(file_path));
        let expected_result = String::from(
            "fa9ecc82691c5939c7872dc3e39d26a50831e122cbcfc1738001c980233e213dc\
            e9e16feb07bdfb93a60ea73e6fa90aca9ce6dd56e5b0626224627b6bc3ad278",
        );
        assert!(checksum.is_ok());
        assert_eq!(expected_result, checksum.unwrap());

        Ok(())
    }
}
