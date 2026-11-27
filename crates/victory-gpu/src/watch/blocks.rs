//! The sets of blocks that the guards of view memory keep (`Guards` in the
//! parent module), as bits.

use std::collections::BTreeMap;

/// A set of block numbers kept as bits, 65,536 blocks (4 GiB of addresses)
/// to a chunk, the chunks in address order.
///
/// The guards' sets were ordered sets of numbers. A frame asks about some
/// 13 GB of view memory, 200,000 blocks, nearly all of them clean, and
/// reading the clean blocks of each asked range off such a set, then
/// looking single blocks up in it, was 6 ms a frame of the thread that
/// submits (docs/FINDINGS.md 2.10: the lookups, and `guard` itself). As
/// bits a range that is all clean is a few words counted.
pub(super) struct Blocks {
    chunks: BTreeMap<usize, Box<[u64; Blocks::WORDS]>>,
    len: usize,
}

impl Blocks {
    /// Blocks to a chunk, as a shift.
    const SHIFT: usize = 16;
    const WORDS: usize = 1 << (Self::SHIFT - 6);

    pub(super) const fn new() -> Self {
        Self {
            chunks: BTreeMap::new(),
            len: 0,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn contains(&self, block: &usize) -> bool {
        self.chunks
            .get(&(block >> Self::SHIFT))
            .is_some_and(|words| words[(block >> 6) & (Self::WORDS - 1)] >> (block & 63) & 1 != 0)
    }

    /// Whether the block was not in the set before.
    pub(super) fn insert(&mut self, block: usize) -> bool {
        let words = self
            .chunks
            .entry(block >> Self::SHIFT)
            .or_insert_with(|| Box::new([0; Self::WORDS]));
        let (word, bit) = (
            &mut words[(block >> 6) & (Self::WORDS - 1)],
            1u64 << (block & 63),
        );
        let new = *word & bit == 0;
        *word |= bit;
        self.len += usize::from(new);
        new
    }

    /// Whether the block was in the set.
    pub(super) fn remove(&mut self, block: &usize) -> bool {
        let Some(words) = self.chunks.get_mut(&(block >> Self::SHIFT)) else {
            return false;
        };
        let (word, bit) = (
            &mut words[(block >> 6) & (Self::WORDS - 1)],
            1u64 << (block & 63),
        );
        let was = *word & bit != 0;
        *word &= !bit;
        self.len -= usize::from(was);
        was
    }

    /// Adds every block of `blocks` (a range of memory that is there: every
    /// chunk it touches is made).
    pub(super) fn extend(&mut self, blocks: std::ops::Range<usize>) {
        let (mut at, to) = (blocks.start, blocks.end);
        while at < to {
            let chunk = at >> Self::SHIFT;
            let base = chunk << Self::SHIFT;
            let end = to.min(base + (1 << Self::SHIFT));
            let words = self
                .chunks
                .entry(chunk)
                .or_insert_with(|| Box::new([0; Self::WORDS]));
            while at < end {
                let w = (at - base) >> 6;
                let first = base + (w << 6);
                let mut mask = !0u64 << (at - first);
                if first + 64 > end {
                    mask &= !0u64 >> (first + 64 - end);
                }
                self.len += (mask & !words[w]).count_ones() as usize;
                words[w] |= mask;
                at = (first + 64).min(end);
            }
        }
    }

    /// Calls `each` for every word that holds blocks of `blocks`, in order:
    /// the number of the word's first block, the word, and the bits of it
    /// that lie in the range. Only the chunks there are are looked at, so
    /// the range may be as long as the address space.
    fn words(&self, blocks: std::ops::Range<usize>, mut each: impl FnMut(usize, u64, u64)) {
        let (from, to) = (blocks.start, blocks.end);
        if from >= to {
            return;
        }
        for (&chunk, words) in self
            .chunks
            .range(from >> Self::SHIFT..=(to - 1) >> Self::SHIFT)
        {
            let base = chunk << Self::SHIFT;
            let (lo, hi) = (from.max(base), to.min(base + (1 << Self::SHIFT)));
            for w in (lo - base) >> 6..=(hi - 1 - base) >> 6 {
                let first = base + (w << 6);
                let mut mask = !0u64;
                if first < lo {
                    mask &= !0u64 << (lo - first);
                }
                if first + 64 > hi {
                    mask &= !0u64 >> (first + 64 - hi);
                }
                each(first, words[w], mask);
            }
        }
    }

    /// How many blocks of `blocks` are in the set.
    pub(super) fn count(&self, blocks: std::ops::Range<usize>) -> usize {
        let mut n = 0;
        self.words(blocks, |_, word, mask| {
            n += (word & mask).count_ones() as usize
        });
        n
    }

    /// The blocks of `blocks` that are in the set, in order.
    pub(super) fn within(&self, blocks: std::ops::Range<usize>) -> Vec<usize> {
        let mut found = Vec::new();
        self.words(blocks, |first, word, mask| {
            let mut bits = word & mask;
            while bits != 0 {
                found.push(first + bits.trailing_zeros() as usize);
                bits &= bits - 1;
            }
        });
        found
    }

    /// The first block of `[from, to)` that is not in the set; `to` if
    /// they all are.
    pub(super) fn first_out(&self, from: usize, to: usize) -> usize {
        let mut at = from;
        while at < to {
            let chunk = at >> Self::SHIFT;
            let Some(words) = self.chunks.get(&chunk) else {
                return at;
            };
            let base = chunk << Self::SHIFT;
            let end = to.min(base + (1 << Self::SHIFT));
            while at < end {
                let w = (at - base) >> 6;
                let first = base + (w << 6);
                let out = !words[w] & (!0u64 << (at - first));
                if out != 0 {
                    return (first + out.trailing_zeros() as usize).min(to);
                }
                at = first + 64;
            }
        }
        to
    }
}

#[cfg(test)]
mod tests {
    use super::super::BLOCK;
    use super::Blocks;

    /// The bit set answers as an ordered set of the same numbers does,
    /// across the ends of words and of chunks.
    #[test]
    fn the_block_set_answers_as_an_ordered_set_does() {
        use std::collections::BTreeSet;
        let mut bits = Blocks::new();
        let mut set = BTreeSet::new();
        let chunk = 1usize << Blocks::SHIFT;
        // Numbers around the ends of words and chunks, in chunks far apart.
        let bases = [0, chunk - 130, 5 * chunk - 70, (1 << 30) + chunk - 3];
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |n: usize| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize % n
        };
        for step in 0..4000 {
            let at = bases[next(bases.len())] + next(200);
            match next(4) {
                0 => assert_eq!(bits.insert(at), set.insert(at)),
                1 => assert_eq!(bits.remove(&at), set.remove(&at)),
                2 => {
                    let n = next(150);
                    bits.extend(at..at + n);
                    set.extend(at..at + n);
                }
                _ => assert_eq!(bits.contains(&at), set.contains(&at)),
            }
            assert_eq!(bits.is_empty(), set.is_empty());
            if step % 16 == 0 {
                let from = bases[next(bases.len())] + next(200);
                let to = from + next(3 * chunk);
                let want: Vec<usize> = set.range(from..to).copied().collect();
                assert_eq!(bits.within(from..to), want);
                assert_eq!(bits.count(from..to), want.len());
                assert_eq!(
                    bits.first_out(from, to),
                    (from..to).find(|b| !set.contains(b)).unwrap_or(to)
                );
            }
        }
        // A range as long as the address space: only the chunks there are
        // are looked at.
        let all = 0..usize::MAX / BLOCK;
        assert_eq!(bits.within(all.clone()).len(), set.len());
        assert_eq!(bits.count(all), set.len());
    }
}
