//! UUID v7 generator.

use uuid::Uuid;

/// Generates time-ordered UUIDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdGenerator;

impl IdGenerator {
    /// Creates a new generator.
    pub const fn new() -> Self {
        Self
    }

    /// Generates a fresh UUID v7.
    pub fn next(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_ordered() {
        let g = IdGenerator::new();
        let a = g.next();
        let b = g.next();
        assert_ne!(a, b);
        // UUID v7 embeds timestamp; later IDs sort lexicographically by time.
        assert!(b.as_bytes() >= a.as_bytes(), "v7 IDs should be time-ordered");
    }
}