//! questionable tree-shaped thing
#![allow(clippy::precedence)]

/// an [`Iterator`] over a number of the most significant bits in an unsigned integer
///
/// this goes most significant to least significant. if the range is larger than the number of
/// bits, it will act like the remaining items are unset bits
#[must_use = "iterators do not do anything until consumed"]
#[derive(Debug, Clone)]
pub struct BitRangeIter<T> {
    inner: T,
    range: usize,
}

macro_rules! bitrange_impl {
    ($($type:ident),*) => {$(
        impl From<($type, usize)> for BitRangeIter<$type> {
           fn from(item: ($type, usize)) -> Self {
                Self {
                    inner: item.0,
                    range: item.1,
                }
            }
        }
        impl Iterator for BitRangeIter<$type> {
            type Item = bool;
            fn next(&mut self) -> Option<Self::Item> {
                if self.range == 0 {
                    return None;
                }
                self.range -= 1;

                let is_set = self.inner & 1 << $type::BITS - 1;
                self.inner <<= 1;
                Some(is_set != 0)
            }
        }
    )*}
}

bitrange_impl!(u8, u16, u32, u64, u128);

/// a binary tree where branches get marked based on where an iterator of bools ends
///
/// probably only useful when [`BitRangeIter`] is used as the iterator
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MarkTree {
    #[default]
    AllUnmarked,
    AllMarked,
    Branch(Box<MarkTree>, Box<MarkTree>),
}

impl MarkTree {
    /// create a new tree
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// mark the position in the tree dictated by an iterator
    pub fn mark(&mut self, mut bits: impl Iterator<Item = bool>) {
        let new = if let Some(bit) = bits.next() {
            match self {
                Self::AllMarked => {
                    return;
                }
                Self::AllUnmarked => {
                    let mut deeper = Self::new();
                    deeper.mark(bits);
                    let mut other = Self::new();

                    if bit {
                        std::mem::swap(&mut deeper, &mut other);
                    }
                    Self::Branch(Box::new(deeper), Box::new(other))
                }
                Self::Branch(a, b) => {
                    let deeper = if bit { b } else { a };
                    deeper.mark(bits);
                    return;
                }
            }
        } else {
            Self::AllMarked
        };
        _ = std::mem::replace(self, new);
    }

    /// clean up branches that are entirely marked or unmarked
    pub fn optimize(&mut self) {
        if let Self::Branch(a, b) = self {
            a.optimize();
            b.optimize();
            if matches!(&**a, Self::AllUnmarked | Self::AllMarked) && a == b {
                // this is only reachable when cheap to clone since this is a leaf
                let new = (**a).clone();
                _ = std::mem::replace(self, new);
            }
        }
    }

    fn walk(&self, path: &mut Vec<bool>, callback: &mut impl FnMut(&Self, &[bool])) {
        callback(self, path);

        if let Self::Branch(a, b) = self {
            path.push(false);
            a.walk(path, callback);
            path.pop();
            path.push(true);
            b.walk(path, callback);
            path.pop();
        }
    }

    /// walk through the tree, calling a callback function on every node
    ///
    /// the callback function is passed the current node and the path taken to get there
    pub fn traverse(&self, mut callback: impl FnMut(&Self, &[bool])) {
        self.walk(&mut vec![], &mut callback);
    }

    pub fn iter<T>(&self) -> MarkTreeIter<'_, T>
    where
        T: for<'a> From<&'a [bool]>,
    {
        MarkTreeIter::<T> {
            stack: vec![(self, TreePos::Root)],
            path: vec![],
            phantom: std::marker::PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
enum TreePos {
    Root,
    Branched { position: bool, level: usize },
}

#[derive(Debug, Clone)]
#[must_use = "iterators do not do anything until consumed"]
pub struct MarkTreeIter<'a, T> {
    stack: Vec<(&'a MarkTree, TreePos)>,
    path: Vec<bool>,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T> Iterator for MarkTreeIter<'a, T>
where
    T: for<'b> From<&'b [bool]>,
{
    type Item = (&'a MarkTree, T);

    fn next(&mut self) -> Option<Self::Item> {
        let (tree, treepos) = self.stack.pop()?;
        if let TreePos::Branched { position, level } = treepos {
            self.path.truncate(level);
            self.path.push(position);
        }

        if let MarkTree::Branch(a, b) = tree {
            let level = if let TreePos::Branched { level, .. } = treepos {
                level + 1
            } else {
                0
            };
            self.stack.push((
                b,
                TreePos::Branched {
                    position: true,
                    level,
                },
            ));
            self.stack.push((
                a,
                TreePos::Branched {
                    position: false,
                    level,
                },
            ));
        }

        Some((tree, T::from(&self.path)))
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::{BitRangeIter, MarkTree};

    #[test]
    fn range_known() {
        let res: Vec<_> = BitRangeIter::from((b'h', 10)).map(u8::from).collect();
        assert_eq!(res, [0, 1, 1, 0, 1, 0, 0, 0, 0, 0]);

        let res: Vec<_> = BitRangeIter::from((1929445575u32, 7))
            .enumerate()
            .filter_map(|(n, b)| b.then_some(n))
            .collect();
        assert_eq!(res, [1, 2, 3, 6]);

        let mut iter = BitRangeIter::from((1u128, 128));
        assert_eq!(iter.clone().count(), 128);
        assert_eq!(iter.next(), Some(false));
        assert_eq!(iter.last(), Some(true));
    }

    #[test]
    fn tree_dedup() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b11000000u8, 3)));
        let old = tree.clone();
        tree.mark(BitRangeIter::from((0b11010100u8, 6)));
        assert_eq!(old, tree);
        tree.mark(BitRangeIter::from((0b11000000u8, 2)));
        let mut new = MarkTree::new();
        new.mark(BitRangeIter::from((0b11000000u8, 2)));
        assert_eq!(tree, new);
    }

    #[test]
    fn tree_optimize() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b01010000u8, 4)));
        tree.mark(BitRangeIter::from((0b01100000u8, 4)));
        tree.mark(BitRangeIter::from((0b01110000u8, 4)));
        let mut simple = MarkTree::new();
        simple.mark(BitRangeIter::from((0b01010000u8, 4)));
        simple.mark(BitRangeIter::from((0b01100000u8, 3)));

        assert_ne!(tree, simple);
        tree.optimize();
        assert_eq!(tree, simple);
    }

    #[test]
    fn tree_traverse() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b1010000000000000u16, 4)));

        tree.traverse(|node, path| {
            if node == &MarkTree::AllMarked {
                assert_eq!(path, [true, false, true, false]);
            } else {
                assert_ne!(path, [true, false, true, false]);
            }
        });
    }

    #[test]
    fn iter_traverse() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((7882829279673712640u64, 32)));
        tree.mark(BitRangeIter::from((7523377975159973992u64, 64)));
        let mut iter = tree.iter::<Vec<bool>>();
        tree.traverse(|tree, path| {
            let (itree, ipath) = iter.next().unwrap();
            assert_eq!(tree, itree);
            assert_eq!(path, ipath);
        });

        assert_eq!(iter.next(), None);
    }
}
