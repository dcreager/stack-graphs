// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

//! Provides several builders for constructing [snapshots][crate::Snapshot].

use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use thiserror::Error;

use crate::EntryKind;
use crate::Snapshot;
use crate::Tree;
use crate::ID;

/// Builds up a [`Snapshot`] from a list of trees.
///
/// Note that this builder is strongly tied to the structure of a snapshot.  For instance, you must
/// not add multiple trees with the same tree ID—it is your responsibility to detect if the
/// snapshot contains multiple subdirectories with the same contents, and only add the
/// corresponding tree once.  This crate also provides [`RelativePathBuilder`], which is more
/// ergonomic to use for a more typically recursive directory listing, where you have the _full,
/// nested_ path if each file within the snapshot.
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

/// Builds up a [`Snapshot`] from a list of files and their _full_ nested path within the snapshot
/// root.
#[derive(Default)]
pub struct RelativePathBuilder {
    root: PendingTree,
    tree_ids: HashMap<Utf8PathBuf, ID>,
}

#[derive(Default)]
struct PendingTree {
    entries: HashMap<Utf8PathBuf, PendingTreeEntry>,
}

enum PendingTreeEntry {
    File(ID),
    Tree(Box<PendingTree>),
}

/// An error that can occur with using a [`RelativePathBuilder`].
#[derive(Debug, Error)]
pub enum RelativePathBuilderError {
    #[error("Path {} already exists", .path)]
    DuplicatePath { path: String },
    #[error("Two directories have same ID {} but are not equal", .id)]
    InconsistentDirectories { id: ID },
    #[error("Path {} is invalid", .path)]
    InvalidPath { path: String },
    #[error("No tree ID provided for {}", .path)]
    MissingTreeID { path: String },
    #[error(transparent)]
    SnapshotBuilderError(#[from] SnapshotBuilderError),
    #[error(transparent)]
    TreeError(#[from] crate::TreeError),
}

impl RelativePathBuilder {
    /// Creates a new empty `RelativePathBuilder`.
    pub fn new() -> RelativePathBuilder {
        RelativePathBuilder::default()
    }

    /// Adds a new directory with the given full path.  Returns an error if there is already a file
    /// or directory with the same name, or if any of the names of any of the new entry's parents
    /// conflict with an existing file.
    ///
    /// Note that you do not _have_ to call this method for the parent directories of the files
    /// that you add. (You can if you want to, it's just not mandatory.) The builder will create
    /// those for you automatically as part of adding files. You only _need_ to call this method to
    /// add an _empty_ directory (since there won't be any children that would cause us to add it
    /// implicitly).
    ///
    /// All trees in a snapshot must have an [`ID`]. If you compile this crate with the `generate`
    /// feature, we can calculate an [`ID`] for this directory for you. (This will happen when you
    /// call the [`build`][Self::build] method—i.e., once we know all of the directory's contents.)
    /// If you do not enable the `generate` feature (or if you don't want to use our auto-generated
    /// [`ID`]s), then you must call [`set_tree_id`][Self::set_tree_id] to provide your own [`ID`]
    /// for each tree (including the ones automatically added for the parents of each file).
    pub fn add_directory<P: AsRef<Utf8Path>>(
        &mut self,
        full_path: P,
    ) -> Result<(), RelativePathBuilderError> {
        let full_path = full_path.as_ref();
        let parent = self.containing_directory(full_path)?;
        let child_name = file_name(full_path, full_path)?;
        match parent.entries.entry(child_name.into()) {
            Entry::Vacant(entry) => {
                let child = PendingTree {
                    entries: HashMap::default(),
                };
                entry.insert(PendingTreeEntry::Tree(Box::new(child)));
            }
            Entry::Occupied(entry) => {
                if matches!(entry.get(), PendingTreeEntry::File(_)) {
                    return Err(RelativePathBuilderError::DuplicatePath {
                        path: full_path.as_str().into(),
                    });
                }
            }
        };
        Ok(())
    }

    /// Adds a new file with the given full path and ID.  Automatically adds directories for each
    /// of the file's parents.  Returns an error if there is already a file or directory with the
    /// same name, or if any of the names of any of the new entry's parents conflict with an
    /// existing file.
    pub fn add_file<P: AsRef<Utf8Path>>(
        &mut self,
        full_path: P,
        id: ID,
    ) -> Result<(), RelativePathBuilderError> {
        let full_path = full_path.as_ref();
        let parent = self.containing_directory(full_path)?;
        let child_name = file_name(full_path, full_path)?;
        match parent.entries.entry(child_name.into()) {
            Entry::Vacant(entry) => entry.insert(PendingTreeEntry::File(id)),
            Entry::Occupied(_) => {
                return Err(RelativePathBuilderError::DuplicatePath {
                    path: full_path.as_str().into(),
                })
            }
        };
        Ok(())
    }

    /// Records a predetermined tree ID for a directory.
    pub fn set_tree_id<P: AsRef<Utf8Path>>(&mut self, full_path: P, id: ID) {
        let full_path = full_path.as_ref();
        self.tree_ids.insert(full_path.into(), id);
    }

    /// Returns the `PendingTree` that is the immediate parent of `full_path`.
    fn containing_directory(
        &mut self,
        full_path: &Utf8Path,
    ) -> Result<&mut PendingTree, RelativePathBuilderError> {
        // If full_path belongs to the root of the snapshot, return it directly.
        let full_parent_path = match full_path.parent() {
            Some(full_parent_path) if !full_parent_path.as_str().is_empty() => full_parent_path,
            _ => return Ok(&mut self.root),
        };

        // Otherwise grab the grandparent directory, and add the parent directory to it if needed.
        let grandparent = self.containing_directory(full_parent_path)?;
        let parent_name = file_name(full_path, full_parent_path)?;
        let parent_direntry = match grandparent.entries.entry(parent_name.into()) {
            Entry::Vacant(entry) => {
                let parent = PendingTree {
                    entries: HashMap::default(),
                };
                entry.insert(PendingTreeEntry::Tree(Box::new(parent)))
            }
            Entry::Occupied(entry) => match entry.into_mut() {
                entry @ PendingTreeEntry::Tree(_) => entry,
                PendingTreeEntry::File(_) => {
                    return Err(RelativePathBuilderError::DuplicatePath {
                        path: full_parent_path.as_str().into(),
                    })
                }
            },
        };
        match parent_direntry {
            PendingTreeEntry::Tree(parent) => Ok(parent.as_mut()),
            _ => unreachable!(),
        }
    }

    pub fn build(self) -> Result<Snapshot, RelativePathBuilderError> {
        let RelativePathBuilder { root, tree_ids } = self;
        let mut full_path = Utf8PathBuf::new();
        let mut built_trees = HashMap::new();
        let root_id = build_tree(&tree_ids, &mut full_path, &mut built_trees, root)?;
        let mut snapshot = SnapshotBuilder::new();
        for (id, tree) in built_trees.drain() {
            snapshot.add_tree(id, tree)?;
        }
        let snapshot = snapshot.with_id(root_id)?;
        Ok(snapshot)
    }
}

fn file_name<'a>(
    full_path: &'a Utf8Path,
    path: &'a Utf8Path,
) -> Result<&'a Utf8Path, RelativePathBuilderError> {
    path.file_name()
        .map(Utf8Path::new)
        .ok_or_else(|| RelativePathBuilderError::InvalidPath {
            path: full_path.as_str().into(),
        })
}

fn build_tree(
    tree_ids: &HashMap<Utf8PathBuf, ID>,
    full_path: &mut Utf8PathBuf,
    built_trees: &mut HashMap<ID, Tree>,
    tree: PendingTree,
) -> Result<ID, RelativePathBuilderError> {
    let mut built = Tree::new();
    for (name, entry) in tree.entries {
        match entry {
            PendingTreeEntry::File(id) => built.add_file(name.into_string(), id)?,
            PendingTreeEntry::Tree(mut child) => {
                full_path.push(&name);
                let child_id = build_tree(
                    tree_ids,
                    full_path,
                    built_trees,
                    std::mem::take(child.as_mut()),
                )?;
                full_path.pop();
                built.add_subdirectory(name.into_string(), child_id)?;
            }
        }
    }

    let id = match tree_ids.get(full_path) {
        Some(id) => id.clone(),
        #[cfg(feature = "generate")]
        None => ID::generate_tree_id(&built),
        #[cfg(not(feature = "generate"))]
        None => {
            return Err(RelativePathBuilderError::MissingTreeID {
                path: full_path.to_string(),
            }
            .into());
        }
    };

    match built_trees.entry(id.clone()) {
        Entry::Vacant(entry) => {
            entry.insert(built);
        }
        Entry::Occupied(entry) => {
            if entry.get() != &built {
                return Err(RelativePathBuilderError::InconsistentDirectories { id }.into());
            }
        }
    };

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    #[test]
    fn can_create_snapshot() {
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/b/d.py", ID::from("[d.py]")).unwrap();
        builder.set_tree_id("", ID::from("[root]"));
        builder.set_tree_id("a", ID::from("[a]"));
        builder.set_tree_id("a/b", ID::from("[b]"));
        let snapshot = builder.build().unwrap();
        assert_eq!(
            indoc! {"
              root [root]

              tree [a]
                b tree [b]

              tree [b]
                c.py file [c.py]
                d.py file [d.py]

              tree [root]
                a tree [a]
            "},
            snapshot.render().to_string(),
        );
    }

    #[cfg(feature = "generate")]
    #[test]
    fn can_create_snapshot_with_generated_ids() {
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/b/d.py", ID::from("[d.py]")).unwrap();
        let snapshot = builder.build().unwrap();
        assert_eq!(
            indoc! {"
              root v0:73aa6fd8fa53e97a7c539803402b1db8a62d7486d560832b01d7029a913cc23a

              tree v0:73aa6fd8fa53e97a7c539803402b1db8a62d7486d560832b01d7029a913cc23a
                a tree v0:7a347439b65849a05f364842edad5ebf5aaaab212405e7e2cb13c36f6079f431

              tree v0:7a347439b65849a05f364842edad5ebf5aaaab212405e7e2cb13c36f6079f431
                b tree v0:ea08917465f1557745c15000c34a9475ca724ec4c20d70266998d34f0fec6b54

              tree v0:ea08917465f1557745c15000c34a9475ca724ec4c20d70266998d34f0fec6b54
                c.py file [c.py]
                d.py file [d.py]
            "},
            snapshot.render().to_string(),
        );
    }

    #[test]
    fn can_create_snapshot_with_identical_directories() {
        // Note that a/b and a/q SHOULD have the same tree ID, since they have the same contents.
        // But if you give them different IDs, that's not an _error_, it just means that you'll
        // lose out on some potential sharing of analysis results for anything keyed by tree ID.
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/b/d.py", ID::from("[d.py]")).unwrap();
        builder.add_file("a/q/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/q/d.py", ID::from("[d.py]")).unwrap();
        builder.set_tree_id("", ID::from("[root]"));
        builder.set_tree_id("a", ID::from("[a]"));
        builder.set_tree_id("a/b", ID::from("[b]"));
        builder.set_tree_id("a/q", ID::from("[q]"));
        let snapshot = builder.build().unwrap();
        assert_eq!(
            indoc! {"
              root [root]

              tree [a]
                b tree [b]
                q tree [q]

              tree [b]
                c.py file [c.py]
                d.py file [d.py]

              tree [q]
                c.py file [c.py]
                d.py file [d.py]

              tree [root]
                a tree [a]
            "},
            snapshot.render().to_string(),
        );
    }

    #[cfg(feature = "generate")]
    #[test]
    fn can_create_snapshot_with_overlapping_generated_ids() {
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/b/d.py", ID::from("[d.py]")).unwrap();
        builder.add_file("a/q/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/q/d.py", ID::from("[d.py]")).unwrap();
        let snapshot = builder.build().unwrap();
        assert_eq!(
            indoc! {"
              root v0:1c6dd3217c2aa889f8b6fd4fa6d087413cf041dd5777afb88acc78a0e07a012c

              tree v0:1c6dd3217c2aa889f8b6fd4fa6d087413cf041dd5777afb88acc78a0e07a012c
                a tree v0:e64ae3fdb29b0f9c18a9338f38967e697e017085b3b38d549e82fda45ad83c5c

              tree v0:e64ae3fdb29b0f9c18a9338f38967e697e017085b3b38d549e82fda45ad83c5c
                b tree v0:ea08917465f1557745c15000c34a9475ca724ec4c20d70266998d34f0fec6b54
                q tree v0:ea08917465f1557745c15000c34a9475ca724ec4c20d70266998d34f0fec6b54

              tree v0:ea08917465f1557745c15000c34a9475ca724ec4c20d70266998d34f0fec6b54
                c.py file [c.py]
                d.py file [d.py]
            "},
            snapshot.render().to_string(),
        );
    }

    #[cfg(feature = "generate")]
    #[test]
    fn cannot_create_snapshot_with_inconsistent_file_and_tree() {
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c", ID::from("[c]")).unwrap();
        let error = builder.add_directory("a/b/c");
        assert!(matches!(
            error,
            Err(RelativePathBuilderError::DuplicatePath { .. })
        ));
    }

    #[test]
    fn cannot_create_snapshot_with_inconsistent_overlapping_ids() {
        // Note that a/b and a/q SHOULD NOT have the same tree ID, since they have different
        // contents.  This is a proper error.
        let mut builder = RelativePathBuilder::new();
        builder.add_file("a/b/c.py", ID::from("[c.py]")).unwrap();
        builder.add_file("a/b/d.py", ID::from("[d.py]")).unwrap();
        builder.add_file("a/q/c.py", ID::from("[c.py]")).unwrap();
        builder.set_tree_id("", ID::from("[root]"));
        builder.set_tree_id("a", ID::from("[a]"));
        builder.set_tree_id("a/b", ID::from("[OVERLAP]"));
        builder.set_tree_id("a/q", ID::from("[OVERLAP]"));
        let error = builder.build();
        assert!(matches!(
            error,
            Err(RelativePathBuilderError::InconsistentDirectories { .. })
        ));
    }
}
