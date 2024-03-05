// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

//! Provides several builders for constructing [snapshots][crate::Snapshot].

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::EntryKind;
use crate::Snapshot;
use crate::Tree;
use crate::ID;

/// Builds up a [`Snapshot`] from a list of trees.
///
/// Note that this builder is strongly tied to the structure of a snapshot.  For instance, you must
/// not add multiple trees with the same tree ID—it is your responsibility to detect if the
/// snapshot contains multiple subdirectories with the same contents, and only add the
/// corresponding tree once.
#[derive(Default)]
pub struct SnapshotBuilder {
    trees: BTreeMap<ID, Tree>,
}

/// An error that occur while using a [`SnapshotBuilder`].
#[derive(Debug, thiserror::Error)]
pub enum SnapshotBuilderError {
    #[error("Snapshot already contains a tree with ID {id}")]
    DuplicateTree { id: ID },
    #[error("Snapshot doesn't contain a tree with ID {id}")]
    MissingTree { id: ID },
    #[error("Snapshot contains trees that aren't mentioned, or mentions nonexistent trees")]
    InconsistentTrees,
}

impl SnapshotBuilder {
    /// Creates a new empty `SnapshotBuilder`.
    pub fn new() -> SnapshotBuilder {
        SnapshotBuilder::default()
    }

    /// Adds a [`Tree`] to this snapshot builder.  You must have already built up the
    /// _complete_ contents of the tree.
    ///
    /// Returns an error if there are any other trees with the same tree ID. (In particular,
    /// that means that it is your responsibility to detect if that the snapshot has multiple
    /// subdirectories with the same contents, and therefore the same tree IDs; and to only add
    /// that tree once.)
    pub fn add_tree<I: Into<ID>>(&mut self, id: I, tree: Tree) -> Result<(), SnapshotBuilderError> {
        let id = id.into();
        if self.trees.contains_key(&id) {
            return Err(SnapshotBuilderError::DuplicateTree { id });
        }
        self.trees.insert(id, tree);
        Ok(())
    }

    /// Performs final verification of the contents of the snapshot, returning the resulting
    /// [`Snapshot`] instance or an error if the snapshot is malformed.
    pub fn with_id<I: Into<ID>>(self, id: I) -> Result<Snapshot, SnapshotBuilderError> {
        let id = id.into();
        if !self.trees.contains_key(&id) {
            return Err(SnapshotBuilderError::MissingTree { id });
        }

        // Verify that every tree entry in every tree exists in the snapshot, and that every
        // tree in the snapshot is mentioned as a tree entry somewhere.
        let defined = self.trees.keys();
        let mut mentioned = BTreeSet::new();
        mentioned.insert(&id);
        for tree in self.trees.values() {
            for entry in tree.entries.values() {
                if entry.kind == EntryKind::Tree {
                    mentioned.insert(&entry.id);
                }
            }
        }
        if !defined.eq(mentioned.into_iter()) {
            return Err(SnapshotBuilderError::InconsistentTrees);
        }

        Ok(Snapshot {
            id,
            trees: self.trees,
        })
    }
}
