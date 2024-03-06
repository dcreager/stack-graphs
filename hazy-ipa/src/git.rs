// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::collections::HashSet;

use crate::builders::SnapshotBuilder;
use crate::Snapshot;
use crate::Tree;
use crate::ID;

/// An error that occur while building a [`Snapshot`] from a git tree.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("error reading git repo")]
    GitError(#[from] ::git2::Error),
    #[error(transparent)]
    SnapshotBuilderError(#[from] crate::builders::SnapshotBuilderError),
    #[error(transparent)]
    TreeError(#[from] crate::TreeError),
}

fn id_for_oid(prefix: &str, oid: git2::Oid) -> ID {
    let oid = oid.as_ref();
    let encoded_len = base16ct::encoded_len(oid);
    let mut result = String::with_capacity(prefix.len() + encoded_len);
    result.push_str(prefix);
    let mut encoded = vec![0u8; encoded_len];
    base16ct::lower::encode(oid, &mut encoded).expect("Invalid length");
    result.push_str(unsafe { std::str::from_utf8_unchecked(&encoded) });
    result.into()
}

impl ID {
    /// Generates a file ID for a git blob.
    pub fn for_git_blob(blob: &git2::Blob) -> ID {
        id_for_oid("git:sha1:", blob.id())
    }

    /// Generates a tree ID for a git tree.
    pub fn for_git_tree(tree: &git2::Tree) -> ID {
        id_for_oid("git:sha1:", tree.id())
    }
}

impl Snapshot {
    /// Generates a snapshot from the contents of a git tree.  The git blob and tree OIDs are used
    /// as the file and tree IDs in the resulting snapshot.
    pub fn from_git_tree(repo: &git2::Repository, tree: &git2::Tree) -> Result<Snapshot, GitError> {
        let mut builder = SnapshotBuilder::new();
        let mut trees_to_visit = vec![tree.to_owned()];
        let mut trees_enqueued = HashSet::new();
        trees_enqueued.insert(tree.id());

        while let Some(git_tree) = trees_to_visit.pop() {
            let mut tree = Tree::new();
            for entry in &git_tree {
                let obj = entry.to_object(repo)?;
                if let Some(subtree) = obj.as_tree() {
                    if trees_enqueued.insert(subtree.id()) {
                        trees_to_visit.push(subtree.to_owned());
                    }
                    let id = ID::for_git_tree(subtree);
                    tree.add_subdirectory(entry.name_bytes(), id)?;
                } else if let Some(blob) = obj.as_blob() {
                    let id = ID::for_git_blob(blob);
                    tree.add_file(entry.name_bytes(), id)?;
                }
                drop(obj);
            }

            let id = ID::for_git_tree(&git_tree);
            builder.add_tree(id, tree)?;
        }

        let root_id = ID::for_git_tree(tree);
        let result = builder.with_id(root_id)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use git2::Oid;
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    struct TestRepo {
        // This needs to be in the struct to make sure the directory is not deleted.
        #[allow(unused)]
        repo_path: TempDir,
        repo: git2::Repository,
        commit_oid: Oid,
    }

    fn clone_repo_from_pack_data(pack_data: &[u8]) -> Result<TestRepo, anyhow::Error> {
        // We're given a git bundle, which is a pack file with a "reference index" at the
        // beginning.  The reference index will look like:
        //
        //     # v2 git bundle
        //     cb5dc0eae67efd44ca632b638d97a1da718cfa48 refs/heads/main
        //
        // It ends with a double-newline.  The pack data itself immediately follows, and starts
        // with a `PACK` magic number.  libgit2 lets us add a packfile to a repository, but it must
        // only include the raw pack data, not the bundle's reference index.

        // Find the commit OID of the first named reference in the bundle index.
        let nl = memchr::memchr(b'\n', pack_data)
            .ok_or_else(|| anyhow::anyhow!("Invalid git bundle"))?;
        let ref_line = &pack_data[nl + 1..];
        let end_of_ref =
            memchr::memchr(b' ', ref_line).ok_or_else(|| anyhow::anyhow!("Invalid git bundle"))?;
        let commit_oid = &ref_line[..end_of_ref];
        let commit_oid = std::str::from_utf8(commit_oid)?;
        let commit_oid = Oid::from_str(commit_oid)?;

        // Skip over the reference index to the raw pack data.
        let nlnl = memchr::memmem::find(pack_data, b"\n\n")
            .ok_or_else(|| anyhow::anyhow!("Invalid git bundle"))?;
        let pack_data = &pack_data[nlnl + 2..];

        // Create a new bare repository and load the pack data into it.
        let repo_path = TempDir::new()?;
        let repo = git2::Repository::init_bare(&repo_path)?;
        {
            let odb = repo.odb()?;
            let mut packwriter = odb.packwriter()?;
            packwriter.write(pack_data)?;
            packwriter.commit()?;
        }

        Ok(TestRepo {
            repo_path,
            repo,
            commit_oid,
        })
    }

    #[test]
    fn can_create_snapshot_from_git_repo() {
        let git_pack = include_bytes!("../../data/typescript_minimal_project.pack");
        let test_repo = clone_repo_from_pack_data(git_pack).unwrap();
        let commit = test_repo.repo.find_commit(test_repo.commit_oid).unwrap();
        let tree = commit.tree().unwrap();
        let snapshot = Snapshot::from_git_tree(&test_repo.repo, &tree).unwrap();
        assert_eq!(
            indoc! {"
              root git:sha1:46f241538c6b28536b2a9c8638810bad440fd928

              tree git:sha1:46f241538c6b28536b2a9c8638810bad440fd928
                typescript_minimal_project tree git:sha1:faa1bb1556fea7aecb2fc6cbe98f36b2cc6777a1

              tree git:sha1:faa1bb1556fea7aecb2fc6cbe98f36b2cc6777a1
                index.ts file git:sha1:3d3b740246d9ef009145ee388f27aa27d3d55e1b
                package.json file git:sha1:3b5e14ed3396a4befc0cf1ddaadef452be8b93db
                tsconfig.json file git:sha1:0967ef424bce6791893e9a57bb952f80fd536e93
                util.ts file git:sha1:9c1d42dfdd959bb00be5cabb8a1a53269a5b3c45
            "},
            snapshot.render().to_string(),
        );
    }
}
