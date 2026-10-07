//! The work list the iterative walks over `Term` and `Symbol` share (docs/design/program.md
//! §13): a last-in, first-out stack whose first entries live inline, so a walk over a shallow
//! value — nearly every term a program holds — allocates nothing, and only a value deeper or
//! wider than the inline part spills to the heap. The walks stay what §13 requires, an
//! explicit work list rather than call-stack recursion; this only decides where its entries
//! live.

/// How many entries a work list keeps inline before it spills to the heap: past the peak a
/// typical atom's arguments reach, at a small fixed cost of stack.
const INLINE: usize = 8;

/// A last-in, first-out work list whose first [`INLINE`] entries live inline and the rest on
/// the heap. Invariant: the heap part is non-empty only while the inline part is full, so
/// whenever the heap part holds an entry it holds the most recent one, and `pop` takes from it
/// first. `T` is a borrow (or a pair of them) into the value being walked, hence `Copy`.
pub(crate) struct WorkList<T: Copy> {
    inline: [T; INLINE],
    len: usize,
    spill: Vec<T>,
}

impl<T: Copy> WorkList<T> {
    /// A work list holding one entry, the walk's root. O(1); allocates nothing.
    pub(crate) fn new(root: T) -> WorkList<T> {
        WorkList {
            inline: [root; INLINE],
            len: 1,
            spill: Vec::new(),
        }
    }

    /// Push an entry: inline while there is room, on the heap after. O(1) amortized.
    pub(crate) fn push(&mut self, entry: T) {
        match self.inline.get_mut(self.len) {
            Some(slot) => {
                *slot = entry;
                self.len += 1;
            }
            None => self.spill.push(entry),
        }
    }

    /// Pop the most recent entry, `None` when the list is empty. O(1).
    pub(crate) fn pop(&mut self) -> Option<T> {
        if let Some(entry) = self.spill.pop() {
            return Some(entry);
        }
        self.len = self.len.checked_sub(1)?;
        self.inline.get(self.len).copied()
    }
}

impl<T: Copy> Extend<T> for WorkList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, entries: I) {
        for entry in entries {
            self.push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{INLINE, WorkList};

    /// Popping returns the entries in reverse push order across the inline/heap boundary,
    /// so a walk sees the order a plain `Vec` stack gives it.
    #[test]
    fn pops_in_last_in_first_out_order_past_the_inline_part() {
        let count = 3 * INLINE + 1;
        let mut list = WorkList::new(0);
        for entry in 1..count {
            list.push(entry);
        }
        let popped: Vec<usize> = std::iter::from_fn(|| list.pop()).collect();
        let expected: Vec<usize> = (0..count).rev().collect();
        assert_eq!(popped, expected);
    }

    /// Interleaved pushes and pops agree with a `Vec` used as a stack, including pushes made
    /// after the list drains back below the inline part.
    #[test]
    fn interleaved_pushes_and_pops_match_a_vec_stack() {
        let mut list = WorkList::new(0_usize);
        let mut reference = vec![0_usize];
        let mut next = 1;
        for round in 0..64 {
            let pushes = (round * 7) % (2 * INLINE + 3);
            let pops = (round * 5) % (2 * INLINE + 1);
            for _ in 0..pushes {
                list.push(next);
                reference.push(next);
                next += 1;
            }
            for _ in 0..pops {
                assert_eq!(list.pop(), reference.pop());
            }
        }
        while let Some(entry) = reference.pop() {
            assert_eq!(list.pop(), Some(entry));
        }
        assert_eq!(list.pop(), None);
    }
}
