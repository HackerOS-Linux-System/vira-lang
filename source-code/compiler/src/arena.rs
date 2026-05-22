use std::alloc::{alloc, dealloc, Layout};
use std::cell::Cell;
use std::ptr::NonNull;

/// Default chunk size: 64 KiB
const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

/// A single chunk in the arena chain.
struct Chunk {
    data: NonNull<u8>,
    capacity: usize,
    #[allow(dead_code)]
    next: Option<Box<Chunk>>,
}

impl Chunk {
    fn new(capacity: usize) -> Self {
        let layout = Layout::from_size_align(capacity, 16).expect("invalid layout");
        // SAFETY: layout is valid and non-zero
        let ptr = unsafe { alloc(layout) };
        let data = NonNull::new(ptr).expect("allocation failed");
        Chunk { data, capacity, next: None }
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.capacity, 16).unwrap();
        // SAFETY: ptr was allocated with same layout
        unsafe { dealloc(self.data.as_ptr(), layout) };
    }
}

/// Chained Arena Allocator.
pub struct Arena {
    head: Box<Chunk>,
    offset: Cell<usize>,
    #[allow(dead_code)]
    chunk_size: usize,
    /// Total bytes allocated across all chunks
    pub allocated: Cell<usize>,
}

impl Arena {
    pub fn new() -> Self {
        Arena::with_chunk_size(DEFAULT_CHUNK_SIZE)
    }

    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Arena {
            head: Box::new(Chunk::new(chunk_size)),
            offset: Cell::new(0),
            chunk_size,
            allocated: Cell::new(0),
        }
    }

    /// Allocate `size` bytes aligned to `align`.
    pub fn alloc_raw(&self, size: usize, align: usize) -> *mut u8 {
        let current_offset = self.offset.get();
        let aligned_offset = align_up(current_offset, align);

        if aligned_offset + size <= self.head.capacity {
            // Fast path: bump pointer
            self.offset.set(aligned_offset + size);
            self.allocated.set(self.allocated.get() + size);
            // SAFETY: offset is within chunk bounds
            unsafe { self.head.data.as_ptr().add(aligned_offset) }
        } else {
            // Slow path: grow chain (requires mutable access — use RefCell in real impl)
            // For transpile output, arena growth is handled by generated Rust code.
            // Here we panic to signal that the chunk size should be increased.
            panic!(
                "Vira arena chunk full ({} bytes used / {} capacity). \
Increase chunk size or restructure allocation.",
aligned_offset, self.head.capacity
            );
        }
    }

    /// Allocate and place a value of type T in the arena.
    pub fn alloc<T>(&self, value: T) -> &mut T {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        let ptr = self.alloc_raw(size, align) as *mut T;
        // SAFETY: ptr is valid and aligned for T
        unsafe {
            ptr.write(value);
            &mut *ptr
        }
    }

    /// Allocate a slice of T values (copy from slice).
    pub fn alloc_slice<T: Copy>(&self, values: &[T]) -> &mut [T] {
        let size = std::mem::size_of::<T>() * values.len();
        let align = std::mem::align_of::<T>();
        let ptr = self.alloc_raw(size, align) as *mut T;
        // SAFETY: ptr is valid, aligned, and has enough room
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), ptr, values.len());
            std::slice::from_raw_parts_mut(ptr, values.len())
        }
    }

    /// Allocate a string slice in the arena.
    pub fn alloc_str(&self, s: &str) -> &str {
        let bytes = self.alloc_slice(s.as_bytes());
        // SAFETY: bytes came from a valid utf-8 str
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    /// Bytes currently used.
    pub fn used(&self) -> usize {
        self.offset.get()
    }

    /// Reset arena (frees all allocations, keeps memory).
    pub fn reset(&mut self) {
        self.offset.set(0);
        self.allocated.set(0);
    }
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}

#[inline(always)]
fn align_up(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}

// ─── Arena scope guard ────────────────────────────────────────────────────────

/// RAII guard that resets an arena when it goes out of scope.
/// Corresponds to `arena { ... }` blocks in Vira.
pub struct ArenaScope<'a> {
    arena: &'a mut Arena,
    saved_offset: usize,
}

impl<'a> ArenaScope<'a> {
    pub fn new(arena: &'a mut Arena) -> Self {
        let saved_offset = arena.offset.get();
        ArenaScope { arena, saved_offset }
    }

    pub fn arena(&self) -> &Arena {
        self.arena
    }
}

impl<'a> Drop for ArenaScope<'a> {
    fn drop(&mut self) {
        self.arena.offset.set(self.saved_offset);
    }
}

// ─── Arena ID system (for code generation) ───────────────────────────────────

/// Used by the compiler to track named arenas in Vira source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArenaId(pub String);

impl ArenaId {
    pub fn new(name: impl Into<String>) -> Self {
        ArenaId(name.into())
    }

    pub fn rust_var_name(&self) -> String {
        format!("__vira_arena_{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_alloc() {
        let arena = Arena::new();
        let x = arena.alloc(42i32);
        assert_eq!(*x, 42);
    }

    #[test]
    fn test_slice_alloc() {
        let arena = Arena::new();
        let s = arena.alloc_slice(&[1u8, 2, 3, 4]);
        assert_eq!(s, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_str_alloc() {
        let arena = Arena::new();
        let s = arena.alloc_str("hello vira");
        assert_eq!(s, "hello vira");
    }

    #[test]
    fn test_multiple_allocs() {
        let arena = Arena::new();
        let a = arena.alloc(1u64);
        let b = arena.alloc(2u64);
        let c = arena.alloc(3u64);
        assert_eq!((*a, *b, *c), (1, 2, 3));
    }

    #[test]
    fn test_reset() {
        let mut arena = Arena::new();
        let _ = arena.alloc(999u32);
        assert!(arena.used() > 0);
        arena.reset();
        assert_eq!(arena.used(), 0);
    }
}
