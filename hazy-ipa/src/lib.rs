// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

//! Implements a _file-level incremental processing model_ for program analysis.
//!
//! This is the incremental processing model used by the [stack graphs][] framework, though it also
//! works for other program analyses, as long as they are incremental at the level of a file. That
//! is, you must be able to analyze each file individually, looking _only_ at the contents of that
//! file in isolation. You can also analyze the contents of each directory, to determine how the
//! analysis results for each file relate to each other given the directory structure of the
//! program snapshot.
//!
//! ## Data model
//!
//! Your analysis will primarily interact with [_snapshots_][Snapshot]. A snapshot consists of a
//! single root [_tree_][Tree] (or “directory”). A tree consists of zero or more
//! [_entries_][TreeEntry], each of which maps a distinct **_filename_** to a
//! [_file_][EntryKind::File] or a [_subdirectory_][EntryKind::Tree]. Each file has a [_file
//! ID_][ID] that depends only on the contents of the file. A tree has a [_tree ID_][ID] that
//! depends only on the names and ID of each entry. Each snapshot has an optional [_snapshot
//! ID_][ID] that depends only on the (transitive) contents of the snapshot. The contents of a
//! snapshot are immutable; we represent source code that changes over time as distinct snapshots.
//!
//! > Note that these concepts align closely with git, just with different terminology: A commit in
//! > a git repository is a [snapshot][Snapshot]; a git tree is a [tree][Tree]; a git tree OID is a
//! > [tree ID][ID]; git blob is a [file][EntryKind::File]; a git blob OID is a [file ID][ID]; and
//! > a git commit’s [snapshot ID][ID] is either the commit OID or the tree OID of the commit’s
//! > tree.
//! >
//! > The different terminology is purposeful, and conveys that this process is not limited to
//! > processing git commits. For instance, the contents of the git index are also a snapshot. It
//! > has a snapshot ID if we use the commit’s tree OID as the snapshot ID, since adding files to
//! > the index from the working tree will persist them in the git object database (with a tree OID
//! > for the repo root) even though a commit has not been minted yet. Similarly, the current dirty
//! > working tree of a locally cloned git repository is a snapshot, but it does not have a
//! > snapshot ID, since it has not been minted into a commit yet, nor have the dirty files been
//! > added to the local git database.
//!
//! ## Building snapshots
//!
//! We provide several _builders_ for constructing snapshots.
//!
//! The lowest level is the [`SnapshotBuilder`][builders::SnapshotBuilder].  This builder is
//! strongly tied to the structure of a snapshot.  For instance, you must not add multiple trees
//! with the same tree ID—it is your responsibility to detect if the snapshot contains multiple
//! subdirectories with the same contents, and only add the corresponding tree once.
//!
//! The [`RelativePathBuilder`][builders::RelativePathBuilder] is easier to use when you have a
//! recursive directory listing of the snapshot—that is, a list of files with their full nested
//! path within the snapshot root. This builder takes care of building up trees from the directory
//! listing, and importantly, it identifies and deduplicates identical trees for you.
//!
//! [stack graphs]: https://docs.rs/stack-graphs/
//!
//! ## Feature flags
//!
//! This crate supports the following feature flags:
//!
//! - `generate`: Adds methods for generating [`ID`]s for files and trees from their content.

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::collections::BTreeMap;

pub mod builders;

#[cfg(feature = "generate")]
#[cfg_attr(docsrs, doc(cfg(feature = "generate")))]
mod generate;

/// An opaque identifier for a file, tree, or snapshot.  IDs should be derived from content: e.g.,
/// two files with the same content should have the same ID.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ID(String);

impl AsRef<[u8]> for ID {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl AsRef<str> for ID {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl std::ops::Deref for ID {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}", &self.0)
    }
}

impl From<String> for ID {
    fn from(id: String) -> ID {
        ID(id)
    }
}

impl From<&str> for ID {
    fn from(id: &str) -> ID {
        ID(id.to_string())
    }
}

impl PartialEq<&str> for ID {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ID> for &str {
    fn eq(&self, other: &ID) -> bool {
        *self == other.0
    }
}

/// Whether an entry is a tree or a file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntryKind {
    File,
    Tree,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Tree => "tree",
        }
    }
}

/// An entry in a tree.
///
/// Note that we only store the ID each any files and subdirectories in the tree.  These IDs
/// are derived from the corresponding contents, so if any file changes in the tree or any of
/// its recursive subdirectories, the tree's ID will change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    pub kind: EntryKind,
    /// The ID of the entry.  The ID of a subdirectory is derived from the subdirectory's tree.
    /// The ID of a file is derived from the file's contents.
    pub id: ID,
}

impl TreeEntry {
    /// Creates a new file tree entry.
    pub fn file(id: ID) -> TreeEntry {
        TreeEntry {
            kind: EntryKind::File,
            id,
        }
    }

    /// Creates a new tree tree entry.
    pub fn tree(id: ID) -> TreeEntry {
        TreeEntry {
            kind: EntryKind::Tree,
            id,
        }
    }
}

/// A tree in a snapshot: a collection of named files and subdirectories.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Tree {
    entries: BTreeMap<Vec<u8>, TreeEntry>,
}

/// An error that occur while building a tree.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("Tree already contains an entry named {name}")]
    DuplicateEntry { name: String },
}

impl Tree {
    /// Creates a new empty tree.
    pub fn new() -> Tree {
        Tree::default()
    }

    /// Adds a file to this tree.  Returns an error if the tree already contains an entry
    /// with the same name.
    pub fn add_file<N: Into<Vec<u8>>, I: Into<ID>>(
        &mut self,
        name: N,
        id: I,
    ) -> Result<(), TreeError> {
        let name = name.into();
        if self.entries.contains_key(&name) {
            let name = String::from_utf8(name)
                .unwrap_or_else(|e| e.into_bytes().escape_ascii().to_string());
            return Err(TreeError::DuplicateEntry { name });
        }
        self.entries.insert(name, TreeEntry::file(id.into()));
        Ok(())
    }

    /// Adds a subdirectory to this tree.  Returns an error if the tree already contains
    /// an entry with the same name.
    pub fn add_subdirectory<N: Into<Vec<u8>>, I: Into<ID>>(
        &mut self,
        name: N,
        id: I,
    ) -> Result<(), TreeError> {
        let name = name.into();
        if self.entries.contains_key(&name) {
            let name = String::from_utf8(name)
                .unwrap_or_else(|e| e.into_bytes().escape_ascii().to_string());
            return Err(TreeError::DuplicateEntry { name });
        }
        self.entries.insert(name, TreeEntry::tree(id.into()));
        Ok(())
    }

    /// Returns an iterator of the entries in this tree, sorted by their names.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &TreeEntry)> {
        self.entries.iter().map(|(n, e)| (n.as_ref(), e))
    }
}

impl<'a> IntoIterator for &'a Tree {
    type Item = (&'a Vec<u8>, &'a TreeEntry);
    type IntoIter = std::collections::btree_map::Iter<'a, Vec<u8>, TreeEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// An immutable snapshot of code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    id: ID,
    trees: BTreeMap<ID, Tree>,
}

impl Snapshot {
    /// Returns the ID of this snapshot.
    pub fn id(&self) -> &ID {
        &self.id
    }

    /// Returns an iterator of the trees in this snapshot, sorted by their IDs.
    ///
    /// Note that if two or more subdirectories have the same (recursive) contents, they will be
    /// represented by the same tree.  This iterator, however, will only return that tree once.
    /// (The tree's ID will be referenced in all of the parent trees that contain the tree as
    /// a subdirectory.)
    pub fn trees(&self) -> impl Iterator<Item = (&ID, &Tree)> {
        self.trees.iter()
    }

    /// Returns a [`Display`][std::fmt::Display] implementation that renders a human-readable
    /// description of the contents of this snapshot.  This is useful in test cases to verify the
    /// contents of a snapshot.
    pub fn render(&self) -> impl std::fmt::Display + '_ {
        SnapshotRenderer(self)
    }
}

impl<'a> IntoIterator for &'a Snapshot {
    type Item = (&'a ID, &'a Tree);
    type IntoIter = std::collections::btree_map::Iter<'a, ID, Tree>;

    fn into_iter(self) -> Self::IntoIter {
        self.trees.iter()
    }
}

#[doc(hidden)]
pub struct SnapshotRenderer<'a>(&'a Snapshot);

impl<'a> std::fmt::Display for SnapshotRenderer<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "root {}\n", self.0.id)?;
        for (id, dir) in &self.0.trees {
            write!(f, "\ntree {}\n", id)?;
            for (name, entry) in &dir.entries {
                if let Ok(name) = std::str::from_utf8(name) {
                    write!(f, "  {} {} {}\n", name, entry.kind.as_str(), entry.id)?;
                } else {
                    write!(
                        f,
                        "  {} {} {}\n",
                        name.escape_ascii(),
                        entry.kind.as_str(),
                        entry.id
                    )?;
                }
            }
        }
        Ok(())
    }
}
