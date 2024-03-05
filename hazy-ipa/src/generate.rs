// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::io::Read;

use sha2::Digest;
use sha2::Sha256;

use crate::EntryKind;
use crate::Tree;
use crate::ID;

fn finalize_id(prefix: &str, hasher: Sha256) -> ID {
    let hash = hasher.finalize();
    let encoded_len = base16ct::encoded_len(&hash[..]);
    let mut result = String::with_capacity(prefix.len() + encoded_len);
    result.push_str(prefix);
    let mut encoded = vec![0u8; encoded_len];
    base16ct::lower::encode(&hash[..], &mut encoded).expect("Invalid length");
    result.push_str(unsafe { std::str::from_utf8_unchecked(&encoded) });
    result.into()
}

impl ID {
    /// Generates a file ID from the contents of the file that is fully available in memory as a
    /// contiguous byte slice.  Files with the same content are guaranteed to have the same ID.
    ///
    /// The algorithm used to generate the ID might change in future versions, but generated IDs
    /// are guaranteed to include a version prefix that ensures that IDs generated by different
    /// algorithms are never considered equal.  (This means that future algorithm changes might
    /// blow out any caches of analysis results that are keyed on older IDs, but will never
    /// accidentally cause old cached results to be reused for unrelated files.)
    pub fn generate_file_id<S: AsRef<[u8]>>(content: S) -> ID {
        let mut hasher = Sha256::new();
        hasher.update(content.as_ref());
        finalize_id("v0:", hasher)
    }

    /// Generates a file ID from the contents of the file that will be read from a
    /// [`std::io::Read`] instance.  (Unlike [`generate_file_id`][Self::generate_file_id], this
    /// method does not require loading the entire contents of the file into memory to generate its
    /// ID.)  Files with the same content are guaranteed to have the same ID.
    ///
    /// The algorithm used to generate the ID might change in future versions, but generated IDs
    /// are guaranteed to include a version prefix that ensures that IDs generated by different
    /// algorithms are never considered equal.  (This means that future algorithm changes might
    /// blow out any caches of analysis results that are keyed on older IDs, but will never
    /// accidentally cause old cached results to be reused for unrelated files.)
    pub fn generate_file_id_from_reader<R: Read>(mut r: R) -> Result<ID, std::io::Error> {
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 4096];
        loop {
            let bytes_read = r.read(&mut buf[..])?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buf[..bytes_read]);
        }
        Ok(finalize_id("v0:", hasher))
    }

    /// Generates a tree ID from the contents of a [`Tree`].  Trees with the same
    /// content (the same set of child filenames, and the same recursive content for each) are
    /// guaranteed to have the same ID.
    ///
    /// The algorithm used to generate the ID might change in future versions, but generated IDs
    /// are guaranteed to include a version prefix that ensures that IDs generated by different
    /// algorithms are never considered equal.  (This means that future algorithm changes might
    /// blow out any caches of analysis results that are keyed on older IDs, but will never
    /// accidentally cause old cached results to be reused for unrelated files.)
    pub fn generate_tree_id(tree: &Tree) -> ID {
        let mut hasher = Sha256::new();
        for (name, entry) in &tree.entries {
            let name_size = name.len() as u64;
            hasher.update(name_size.to_ne_bytes());
            hasher.update(name);
            hasher.update(match entry.kind {
                EntryKind::File => b"F",
                EntryKind::Tree => b"T",
            });
            let id_size = entry.id.len() as u64;
            hasher.update(id_size.to_ne_bytes());
            hasher.update(&entry.id);
        }
        finalize_id("v0:", hasher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_generate_file_ids() {
        assert_eq!(
            "v0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ID::generate_file_id(b""),
        );
        assert_eq!(
            "v0:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            ID::generate_file_id(b"hello world"),
        );
    }

    #[test]
    fn can_generate_file_ids_from_reader() {
        assert_eq!(
            "v0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ID::generate_file_id_from_reader(b"".as_slice()).unwrap(),
        );
        assert_eq!(
            "v0:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            ID::generate_file_id_from_reader(b"hello world".as_slice()).unwrap(),
        );
    }

    #[test]
    fn can_generate_tree_ids() {
        let mut tree = Tree::new();
        assert_eq!(
            "v0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ID::generate_tree_id(&tree),
        );

        tree.add_file(
            "test-file",
            "v0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        .expect("Error adding file");
        tree.add_subdirectory(
            "test-tree",
            "v0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        .expect("Error adding tree");
        assert_eq!(
            "v0:c202acb72512cc6220a5ac76f3132e81e28d7097fa618c4615200d2d5c4a0a8d",
            ID::generate_tree_id(&tree),
        );
    }
}
