use std::ops::{Deref, DerefMut};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::marker::PhantomData;

/// An AtomicUsize containing a bitpacked `T` .
pub struct Atomic<T: Packable>(AtomicUsize, PhantomData<T>);

/// Specify how to bitpack a value.
pub trait Packable: Sized + Copy {
    unsafe fn encode(val: Self) -> usize;
    unsafe fn decode(val: usize) -> Self;
}

/// An attempt to perform a transaction.
#[must_use]
pub struct Transact<'a, T: Packable> {
    atom: &'a Atomic<T>,
    current: usize,
    new: T,
}

impl<'a, T: Packable> Deref for Transact<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.new
    }
}

impl<'a, T: Packable> DerefMut for Transact<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.new
    }
}

impl<'a, T: Packable> Transact<'a, T> {
    /// Try to commit a transaction to an atomic variable. Return an error containing the current
    /// value on error.
    pub fn commit(self) -> Result<T, usize> {
        unsafe {
            match self.atom.0.compare_exchange_weak(
                self.current, T::encode(self.new),
                AcqRel, Acquire) {
                Err(e) => Err(e),
                Ok(_) => Ok(self.new),
            }
        }
    }
}

impl<T: Packable> Atomic<T> {
    pub fn new(val: T) -> Self {
        Atomic(AtomicUsize::new(unsafe { T::encode(val) }), PhantomData)
    }
    pub fn load(&self, order: Ordering) -> T {
        unsafe { T::decode(self.0.load(order)) }
    }
    /// Perform a transaction (similar fetch_update).
    /// Calls the callback with the current value as a Transact. Callers may mutate this Transact
    /// and commit it. Errors from commit should be raised with '?'.
    pub fn transact<'a, R>(&'a self, mut update: impl FnMut(Transact<'a, T>) -> Result<R, usize>) -> R {
        unsafe {
            let mut value = self.0.load(Acquire);
            loop {
                match update(Transact {
                    atom: self,
                    current: value,
                    new: T::decode(value),
                }) {
                    Err(e) => value = e,
                    Ok(v) => return v,
                }
            }
        }
    }
}