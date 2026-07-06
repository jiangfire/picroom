//! Pagination primitives.

use serde::{Deserialize, Serialize};

/// A page request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageReq {
    /// Page size (1–200).
    pub limit: u32,
    /// Opaque cursor returned by the previous page (if any).
    pub cursor: Option<String>,
}

impl Default for PageReq {
    fn default() -> Self {
        Self {
            limit: 50,
            cursor: None,
        }
    }
}

impl PageReq {
    /// Clamps the limit to `[1, 200]`.
    pub fn with_clamped_limit(mut self) -> Self {
        self.limit = self.limit.clamp(1, 200);
        self
    }
}

/// A page of items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items in this page.
    pub items: Vec<T>,
    /// Cursor for the next page, or `None` if last.
    pub next_cursor: Option<String>,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
    /// Echo of the original request.
    pub req: PageReq,
}

impl<T> Page<T> {
    /// Creates a new page.
    pub const fn new(items: Vec<T>, next_cursor: Option<String>, req: PageReq) -> Self {
        let has_more = next_cursor.is_some();
        Self {
            items,
            next_cursor,
            has_more,
            req,
        }
    }

    /// Returns whether the page is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the number of items in the page.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_marks_has_more_when_cursor_present() {
        let p: Page<u32> = Page::new(vec![1, 2], Some("abc".into()), PageReq::default());
        assert!(p.has_more);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn page_marks_no_more_when_cursor_absent() {
        let p: Page<u32> = Page::new(vec![1], None, PageReq::default());
        assert!(!p.has_more);
        assert!(!p.is_empty());
    }

    #[test]
    fn limit_clamped_to_range() {
        let r = PageReq {
            limit: 999,
            cursor: None,
        }
        .with_clamped_limit();
        assert_eq!(r.limit, 200);

        let r = PageReq {
            limit: 0,
            cursor: None,
        }
        .with_clamped_limit();
        assert_eq!(r.limit, 1);
    }
}