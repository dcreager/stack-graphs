// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2024, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use crate::EntryKind;
use crate::ID;

/// Defines how to analyze source code from a particular family of programming languages.
pub trait LanguageAnalyzer {
    type Result;
    type Additional;
    type Error;

    /// Returns the name of the programming language family that this analyzer handles.
    fn name(&self) -> &'static str;

    /// Returns the version of this language analyzer.  (Cached results are stored separate for
    /// each version of each language analyzer.)
    fn version(&self) -> &'static str;

    /// Determines which operations need to be performed for a [`Snapshot`][crate::Snapshot].  This
    /// method is not cache-aware—you should produce a complete list of operations, and a follow-on
    /// step will determine whether any of those operations have already been performed and cached
    /// previously.
    fn categorize_snapshot(&mut self) -> Result<Vec<Operation<Self::Additional>>, Self::Error>;

    /// Performs an operation, returning its result.
    fn perform_operation(
        &mut self,
        snapshot_id: ID,
        op: &Operation<Self::Additional>,
    ) -> Result<Self::Result, Self::Error>;

    /// Ensures that an operation has been performed, but does not return its result.
    fn ensure_operation_performed(
        &mut self,
        snapshot_id: ID,
        op: &Operation<Self::Additional>,
    ) -> Result<(), Self::Error>;
}

/// Describes an operation that a [`LanguageAnalyzer`] need to perform to analyze the contents of a
/// [`Snapshot`][crate::Snapshot].
///
/// If you are implementing a [`Cache`] of operation results, this type contains all of the data
/// you need to include in the cache key.  (Note that you must have a separate cache for each
/// (version of each) [`LanguageAnalyzer`] that you support.) To help with this, operations
/// implement [`Eq`], [`Hash`], and [`Ord`], and so can be used as keys in [`BTreeMap`]s and
/// [`HashMap`]s.
///
/// Note that this type only _identifies_ the operation; it does not specify what work will be
/// performed when the operation is executed.  (That is specified by the
/// [`perform_operation`][LanguageAnalyzer::perform_operation] method of the particular
/// [`LanguageAnalyzer`] that this operation belongs to.)
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Operation<A> {
    /// Whether this operation will analyze a file or a tree
    pub kind: EntryKind,
    /// The [`ID`] of the file or tree to be analyzed
    pub id: ID,
    /// Any additional data that is needed to perform this operation
    pub additional: A,
}

impl<A> Operation<A> {
    /// Creates a new operation.
    pub fn new(kind: EntryKind, id: ID, additional: A) -> Operation<A> {
        Operation {
            kind,
            id,
            additional,
        }
    }
}

#[derive(Clone, Eq)]
pub struct JSONMetadata {
    value: serde_json::Value,
    canonical: String,
}

impl JSONMetadata {
    pub fn new(
        value: serde_json::Value,
    ) -> Result<JSONMetadata, canonical_json::CanonicalJSONError> {
        let canonical = canonical_json::to_string(&value)?;
        Ok(JSONMetadata { value, canonical })
    }
}

impl std::ops::Deref for JSONMetadata {
    type Target = serde_json::Value;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl std::ops::DerefMut for JSONMetadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl PartialEq<JSONMetadata> for JSONMetadata {
    fn eq(&self, other: &JSONMetadata) -> bool {
        self.canonical == other.canonical
    }
}

impl std::hash::Hash for JSONMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
    }
}

impl Ord for JSONMetadata {
    fn cmp(&self, other: &JSONMetadata) -> std::cmp::Ordering {
        self.canonical.cmp(&other.canonical)
    }
}

impl PartialOrd<JSONMetadata> for JSONMetadata {
    fn partial_cmp(&self, other: &JSONMetadata) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
