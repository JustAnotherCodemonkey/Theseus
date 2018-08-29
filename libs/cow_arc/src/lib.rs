#![no_std]
#![feature(alloc)]

extern crate alloc;
extern crate spin;
extern crate owning_ref;

use alloc::arc::{Arc, Weak};
use spin::Mutex;
use owning_ref::{MutexGuardRef, MutexGuardRefMut};

/// A special form of an `Arc` reference that uses two nested `Arc`s to support a 
/// mechanism similar to copy-on-write or clone-on-write.
/// 
/// Effectively, this type reduces to `Arc<Arc<Mutex<T>>>`.
/// 
/// Unlike regular `Arc`s, which do not permit mutability if there are multiple strong or weak references
/// to the data inside the `Arc`, the `CowArc` type can still allow interior mutability of the data `T`
/// when there are multiple strong or weak references to it. 
/// This works by treating the inner `Arc` reference as the actual reference count,
/// enabling it to differentiate between two states:     
/// * Exclusive:  only a single strong reference to the internal `Arc`, 
///   meaning that it is okay to mutate the data.
/// * Shared:  there are multiple strong references to the internal `Arc`,
///   meaning that it cannot be accessed mutably. 
/// 
/// The inner data `T` is protected by a `Mutex`, allowing it to be borrowed mutably 
/// when the `CowArc` is in the `Exclusive` state only. 
/// The inner data can be borrowed immutably in either state. 
/// 
/// This point of this data type is to encourage deeply copying data that's in the `Shared` state
/// in order to modify it, because deeply copying (cloning) `Shared` data will yield a new instance
/// that starts in the `Exclusive` state by default.
/// 
/// Finally, the `CowArc` type can be "cloned" in two ways:     
/// * using the regular `clone` function, which actually affects the shared state
///   by duplicating the inner reference, meaning that the `CowArc` will be in the shared state after invoking `clone`,
/// * using the [`clone_shallow`](#method.clone_shallow) function, which does not affect the shared state
///   and only duplicates the outer reference. 
#[derive(Debug)]
pub struct CowArc<T> {
    arc: Arc<InnerRef<T>>,
}
impl<T> CowArc<T> {
    /// Crates a new `CowArc` that wraps the given data. 
    /// The new `CowArc` will be in the `Exclusive` state, 
    /// that is, not shared. 
    pub fn new(data: T) -> CowArc<T> {
        CowArc {
            arc: Arc::new(InnerRef {
                inner_arc: Arc::new(Mutex::new(data)),
            })
        }
    }

    /// This acquires the lock on the inner `Mutex` wrapping the data `T`,
    /// and always succeeds because an `CowArc` always allows immutable access to the data,
    /// whether the data is `Shared` or `Exclusive`.
    /// 
    /// The returned value derefs to and can be used exactly like `&T`.
    pub fn lock_as_ref(&self) -> MutexGuardRef<T> {
        MutexGuardRef::new(self.arc.inner_arc.lock())
    }

    /// This acquires the lock on the inner `Mutex` wrapping the data `T` if it succeeds,
    /// which only occurs if this `CowArc` is in the `Shared` state, i.e., 
    /// only a single strong reference to the inner `Arc` is held.
    /// 
    /// The returned value derefs to and can be used exactly like `&mut T`.
    pub fn lock_as_mut(&self) -> Option<MutexGuardRefMut<T>> {
        if self.is_shared() {
            // shared state, should not mutate 
            None
        }
        else {
            Some(MutexGuardRefMut::new(self.arc.inner_arc.lock()))
        }
    }

    /// Downgrades this `CowArc` into a `CowWeak` weak reference.
    pub fn downgrade(this: &CowArc<T>) -> CowWeak<T> {
        CowWeak {
            weak: Arc::downgrade(&this.arc),
        }
    }

    /// Returns `true` if this `CowArc` is in the `Shared` state, 
    /// and `false` if it is in the `Exclusive` state.
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.arc.inner_arc) > 1
    }


    /// Creates a shallow clone of this `CowArc` that **does not** affect its `Shared` state.
    /// This means that it will not change it to `Shared` if it was `Exclusive,
    /// nor will it increase the shared count if it was already `Shared`. 
    /// 
    /// Likewise, dropping the returned reference will not decrement the shared count
    /// nor potentially change its state from `Shared` back to `Exclusive`.
    /// 
    /// This is useful for passing around a duplicate reference 
    /// to the same instance (of the outer reference) 
    /// that will be used somewhere else temporarily, e.g., in the same context,
    /// without marking it as a totally separate shared instance. 
    /// 
    /// The fact that this is different from the `clone` function 
    /// is what differentiates the behavior of `CowArc` from regular `Arc`.
    pub fn clone_shallow(&self) -> CowArc<T> {
        CowArc {
            arc: Arc::clone(&self.arc),
        }
    }
}

impl<T> Clone for CowArc<T> {
    /// Creates a shared reference to `this` `CowArc` 
    /// and returns that shared reference as a new `CowArc`
    /// whose internal reference points to the same data.
    /// 
    /// This increases the shared count of this `CowArc`, 
    /// and the returned new `CowArc` instance will have 
    /// the same shared count and reference the same data.
    fn clone(&self) -> CowArc<T> {
        CowArc {
            arc: Arc::new(
                InnerRef {
                    inner_arc: Arc::clone(&self.arc.inner_arc),
                }
            ),
        }
    }
}




/// A weak reference to a `CowArc`, just like a `Weak` is to an `Arc`.
#[derive(Debug)]
pub struct CowWeak<T> {
    weak: Weak<InnerRef<T>>,
}
impl<T> CowWeak<T> {
    /// Just like `Weak::upgrade()`, attempts to upgrade this `CowWeak`
    /// into a strong reference to the `CowArc` that it points to.
    pub fn upgrade(&self) -> Option<CowArc<T>> {
        self.weak.upgrade().map(|arc| CowArc {
            arc: arc,
        })
    }
}
impl<T> Clone for CowWeak<T> {
    fn clone(&self) -> CowWeak<T> {
        CowWeak {
            weak: self.weak.clone()
        }
    }
}


/// The inner reference inside of a `CowArc`.
#[derive(Debug)]
struct InnerRef<T> {
    inner_arc: Arc<Mutex<T>>,
}