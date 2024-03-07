// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2024, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;

use crate::analysis::LanguageAnalyzer;
use crate::analysis::Operation;
use crate::ID;

pub trait Cache<O, R> {
    fn contains(&self, op: &O) -> bool;
    fn get(&self, op: &O) -> Option<R>;
    fn put(&mut self, op: &O, result: R);
}

pub struct CachedLanguageAnalyzer<L, C> {
    wrapped: L,
    cache: C,
}

impl<L, C> CachedLanguageAnalyzer<L, C> {
    pub fn new(wrapped: L, cache: C) -> CachedLanguageAnalyzer<L, C> {
        CachedLanguageAnalyzer { wrapped, cache }
    }
}

impl<L, C, A, R> LanguageAnalyzer for CachedLanguageAnalyzer<L, C>
where
    L: LanguageAnalyzer<Result = R, Additional = A>,
    C: Cache<Operation<A>, R>,
    R: Clone,
{
    type Result = R;
    type Additional = A;
    type Error = L::Error;

    fn name(&self) -> &'static str {
        self.wrapped.name()
    }

    fn version(&self) -> &'static str {
        self.wrapped.version()
    }

    fn categorize_snapshot(&mut self) -> Result<Vec<Operation<A>>, Self::Error> {
        self.wrapped.categorize_snapshot()
    }

    fn perform_operation(
        &mut self,
        snapshot_id: ID,
        op: &Operation<A>,
    ) -> Result<Self::Result, Self::Error> {
        if let Some(cached) = self.cache.get(op) {
            return Ok(cached);
        }
        let result = self.wrapped.perform_operation(snapshot_id, op)?;
        self.cache.put(op, result.clone());
        Ok(result)
    }

    fn ensure_operation_performed(
        &mut self,
        snapshot_id: ID,
        op: &Operation<A>,
    ) -> Result<(), Self::Error> {
        if self.cache.contains(op) {
            return Ok(());
        }
        let result = self.wrapped.perform_operation(snapshot_id, op)?;
        self.cache.put(op, result);
        Ok(())
    }
}

pub struct NoCache;

impl<O, R> Cache<O, R> for NoCache {
    fn contains(&self, _op: &O) -> bool {
        false
    }

    fn get(&self, _op: &O) -> Option<R> {
        None
    }

    fn put(&mut self, _op: &O, _result: R) {}
}

impl<O, R> Cache<O, R> for BTreeMap<O, R>
where
    O: Clone + Ord,
    R: Clone,
{
    fn contains(&self, op: &O) -> bool {
        self.contains_key(op)
    }

    fn get(&self, op: &O) -> Option<R> {
        self.get(op).cloned()
    }

    fn put(&mut self, op: &O, result: R) {
        self.insert(op.clone(), result);
    }
}

impl<O, R> Cache<O, R> for HashMap<O, R>
where
    O: Clone + Eq + Hash,
    R: Clone,
{
    fn contains(&self, op: &O) -> bool {
        self.contains_key(op)
    }

    fn get(&self, op: &O) -> Option<R> {
        self.get(op).cloned()
    }

    fn put(&mut self, op: &O, result: R) {
        self.insert(op.clone(), result);
    }
}

impl<C, O, R> Cache<O, R> for Arc<Mutex<C>>
where
    C: Cache<O, R>,
{
    fn contains(&self, op: &O) -> bool {
        self.lock().unwrap().contains(op)
    }

    fn get(&self, op: &O) -> Option<R> {
        self.lock().unwrap().get(op)
    }

    fn put(&mut self, op: &O, result: R) {
        self.lock().unwrap().put(op, result);
    }
}

impl<C, O, R> Cache<O, R> for Arc<RwLock<C>>
where
    C: Cache<O, R>,
{
    fn contains(&self, op: &O) -> bool {
        self.read().unwrap().contains(op)
    }

    fn get(&self, op: &O) -> Option<R> {
        self.read().unwrap().get(op)
    }

    fn put(&mut self, op: &O, result: R) {
        self.write().unwrap().put(op, result);
    }
}
