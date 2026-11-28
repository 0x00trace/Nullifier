//! Where the title's instance-culling programs store through their view of
//! the whole heap.
//!
//! Seven compute programs of the title bind an unordered-access view of
//! all of its largest buffer (174 MiB in the world), in every culling list
//! of a frame. A view says nothing of where its shader stores, so each of
//! those dispatches declared the whole buffer as written by shaders. Until
//! such a submission had run and been copied back, everything that reads
//! title memory there waited for it: the fills of nearly every constant
//! buffer, and the refreshes of what the title wrote meanwhile
//! (docs/FINDINGS.md 2.25 to 2.27).
//!
//! The programs store at three kinds of addresses only, and all three come
//! from two small records in the buffer that the title writes before it
//! submits the dispatch. With `H[x]` the dword at byte `x` of the buffer:
//!
//! - a table of one dword for each work group gives `B`, the place of the
//!   group's batch record;
//! - `M = H[B + 8]` is the place of the batch's model record, and
//!   `cnt = H[B + 64]` its instances;
//! - records `r` from `r0 = H[M + 104]` on are touched: at most
//!   `3 * H[M + 128]` and the largest of the five ends `H[M + 108..128]`
//!   past it, or `H[M + 132] + 1` past it;
//! - a counter of each record is added to at `H[B + 36] + 20 * r`, a count
//!   at `H[B + 88] + 16 * r`, and the instances that pass go into up to
//!   five lists at `H[B + 12 + 4 * l]`, 16 bytes for each instance of the
//!   batch at most.
//!
//! [`bound`] reads the records and gives the ranges. `VICTORY_CULL_RANGES`
//! says what is done with them ([`mode`]).

use std::sync::Arc;

use crate::shadow::Shadow;

/// The programs: the hashes in their pipelines' names.
const PROGRAMS: [&str; 7] = [
    "17af15c4015cb4b6",
    "55679a0547649d0b",
    "1c4b3c0547e3e6fa",
    "42f717f1b4d18e94",
    "89d08e561897d4e5",
    "9209777e8face9dd",
    "fdd56d97a806bcca",
];

/// The dword of the first table at which the programs' view of the whole
/// buffer lies, and of the second at which the table of batch records lies.
pub(crate) const WHOLE_AT: u32 = 1480;
pub(crate) const BATCHES_AT: u32 = 0;

/// Bytes from which a view counts as one of a whole buffer.
pub(crate) const WHOLE: u64 = 64 << 20;

/// Ranges of a bound closer than this many bytes are declared as one. The
/// lists of the batches of a dispatch lie one behind the other with a few
/// bytes of padding between them: as ranges of their own they were about
/// 2,000 declarations a frame, each with a compare pass on the card, for
/// what is a few dozen stretches of memory.
const JOINED: usize = 512;

/// The most records and instances a batch is taken to have: more is a
/// record that was not read right.
const MOST_RECORDS: u64 = 1 << 20;
const MOST_INSTANCES: u64 = 1 << 22;

/// What is done with the bound (`VICTORY_CULL_RANGES`): 0, nothing (unset);
/// 1, `check` (diagnostics builds; else as unset): the whole view is
/// declared as before, and the bytes that shaders changed are counted
/// inside and outside the bounds ([`check`]); 2, `1`: the bound's ranges
/// are declared in place of the whole view.
pub(crate) fn mode() -> u8 {
    static MODE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("VICTORY_CULL_RANGES").as_deref() {
        Ok("1") => 2,
        Ok("check") if cfg!(feature = "diagnostics") => 1,
        _ => 0,
    })
}

/// Whether the pipeline named `name` runs one of the programs.
pub(crate) fn is_culling(name: &str) -> bool {
    PROGRAMS.iter().any(|hash| name.contains(hash))
}

/// What a culling dispatch's bound is read from when it is submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Batches {
    /// The buffer the view shows: title address and bytes.
    pub(crate) heap: usize,
    pub(crate) heap_len: usize,
    /// The table of batch records: title address, and its dwords (one for
    /// each work group).
    pub(crate) table: usize,
    pub(crate) groups: u32,
}

/// A culling dispatch's declaration of what its shaders write, made when
/// the list is submitted (`crate::host`'s step).
pub(crate) struct Bounded {
    pub(crate) shadow: Arc<Shadow>,
    /// The view, as it would be declared whole.
    pub(crate) address: usize,
    pub(crate) len: usize,
    pub(crate) batches: Batches,
    /// [`mode`] 1: the view is declared whole by another step, and this
    /// one only tells the check of the bound.
    pub(crate) check: bool,
    /// The ranges declared at the last submission (title addresses).
    pub(crate) found: std::sync::Mutex<Vec<(usize, usize)>>,
}

/// A view as it is declared written by shaders: the copy, the title
/// address and the bytes.
pub(crate) type Declared = (Arc<Shadow>, u64, u64);

thread_local! {
    /// While a culling dispatch binds its views with [`mode`] 2: the views
    /// of a whole buffer it met, whose declarations were withheld.
    static WITHHELD: std::cell::RefCell<Option<Vec<Declared>>> = const { std::cell::RefCell::new(None) };
}

/// A culling dispatch is about to bind its views: the declarations of its
/// views of a whole buffer are withheld ([`withhold`]) until
/// [`bound_views`].
pub(crate) fn binding() {
    WITHHELD.set(Some(Vec::new()));
}

/// The dispatch's views are bound: the declarations that were withheld,
/// for the caller to make, as a bound or whole.
pub(crate) fn bound_views() -> Vec<Declared> {
    WITHHELD.take().unwrap_or_default()
}

/// For an unordered-access view that is about to be declared as written
/// by shaders: whether the declaration is withheld (a view of [`WHOLE`]
/// bytes or more, while a culling dispatch binds).
pub(crate) fn withhold(shadow: &Arc<Shadow>, address: u64, len: u64) -> bool {
    if len < WHOLE {
        return false;
    }
    WITHHELD.with_borrow_mut(|withheld| match withheld {
        Some(views) => {
            views.push((shadow.clone(), address, len));
            true
        }
        None => false,
    })
}

/// The ranges (title addresses, in order, joined where they touch or lie
/// within [`JOINED`] bytes of each other) that hold everything a culling
/// dispatch can store through its view of the whole buffer; `read` copies
/// title memory from an address into a slice and says whether it could.
/// `None` if a record cannot be read or does not lie in the buffer: the
/// caller then declares the whole view.
pub(crate) fn bound(
    batches: &Batches,
    read: impl Fn(usize, &mut [u8]) -> bool,
) -> Option<Vec<(usize, usize)>> {
    let len = batches.heap_len as u64;
    // Dwords `n` from byte `at` of the buffer.
    let dwords = |at: u64, n: usize| -> Option<Vec<u32>> {
        if at.checked_add(4 * n as u64)? > len {
            return None;
        }
        let mut bytes = vec![0u8; 4 * n];
        read(batches.heap + at as usize, &mut bytes).then(|| {
            bytes
                .chunks_exact(4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        })
    };
    let mut table = vec![0u8; 4 * batches.groups as usize];
    if !read(batches.table, &mut table) {
        return None;
    }
    let mut records: Vec<u32> = table
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    records.sort_unstable();
    records.dedup();
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for b in records {
        // The batch record: dwords 2 to 22 of it are read.
        let batch = dwords(u64::from(b), 23)?;
        let (model, lists, counters, instances, counts) = (
            batch[2],
            &batch[3..8],
            batch[9],
            u64::from(batch[16]),
            batch[22],
        );
        // The model record: dwords 26 to 33.
        let m = dwords(u64::from(model) + 104, 8)?;
        let first = u64::from(m[0]);
        let ends = m[1..6].iter().copied().max().map_or(0, u64::from);
        let (stride, tail) = (u64::from(m[6]), u64::from(m[7]));
        let records = (3 * stride + ends).max(tail + 1);
        if records > MOST_RECORDS || instances > MOST_INSTANCES {
            return None;
        }
        let mut add = |from: u64, bytes: u64| -> Option<()> {
            let to = from.checked_add(bytes)?;
            if to > len {
                return None;
            }
            if bytes > 0 {
                ranges.push((from, to));
            }
            Some(())
        };
        add(u64::from(counters) + 20 * first, 20 * records)?;
        add(u64::from(counts) + 16 * first, 16 * records)?;
        for &list in lists {
            if list != u32::MAX {
                add(u64::from(list), 16 * instances)?;
            }
        }
    }
    ranges.sort_unstable();
    let mut joined: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (from, to) in ranges {
        let (from, to) = (batches.heap + from as usize, batches.heap + to as usize);
        match joined.last_mut() {
            Some(last) if from <= last.1 + JOINED => last.1 = last.1.max(to),
            _ => joined.push((from, to)),
        }
    }
    Some(joined)
}

/// With `VICTORY_CULL_RANGES=check` (diagnostics builds): whether the
/// bounds hold what the programs store. The view of the whole buffer is
/// declared as without the switch, so every piece its shaders change comes
/// back from the card (for a copy in the card's memory). Each byte they
/// changed in the buffer is looked up in the bounds and in the ranges that
/// submissions of the last frames declared exactly (other programs' views
/// of parts of the buffer), and the frame line says how many bytes lay in
/// neither. With `VICTORY_CULL_RANGES=1` the line gives the bounds only.
#[cfg(feature = "diagnostics")]
pub(crate) mod check {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    /// Frames a declared range stays in [`ALLOWED`] after its last
    /// declaration: more than pass before a submission is copied back.
    const KEPT_FRAMES: u64 = 16;

    /// The ranges that submissions declared exactly, and the bounds of
    /// culling dispatches, as title addresses joined where they touch:
    /// start to end and the frame last declared in.
    static ALLOWED: Mutex<BTreeMap<usize, (usize, u64)>> = Mutex::new(BTreeMap::new());
    /// Since the last line: bounds made, bounds that could not be made,
    /// the ranges of the bounds and their bytes; bytes shaders changed in
    /// the pieces fetched, and those of them in the buffer and outside
    /// every range of [`ALLOWED`].
    static COUNT: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
    /// The first address found changed outside every range, since the start.
    static FIRST_OUTSIDE: AtomicU64 = AtomicU64::new(0);
    /// The buffer the culling programs' view shows (start and end): only
    /// what shaders change in it is checked against the bounds.
    static HEAP: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

    /// A submission declares `ranges` (title addresses).
    pub(crate) fn allow(ranges: &[(usize, usize)]) {
        let frame = crate::frame::current();
        let mut map = ALLOWED.lock().unwrap_or_else(|e| e.into_inner());
        for &(mut from, mut to) in ranges {
            if from >= to {
                continue;
            }
            let met: Vec<usize> = map
                .range(..=to)
                .rev()
                .take_while(|(_, entry)| entry.0 >= from)
                .map(|(&start, _)| start)
                .collect();
            for start in met {
                if let Some((end, _)) = map.remove(&start) {
                    from = from.min(start);
                    to = to.max(end);
                }
            }
            map.insert(from, (to, frame));
        }
    }

    /// Whether a submission of the last frames declared all of `[from, to)`,
    /// or the bytes lie outside the buffer the bounds are for.
    pub(crate) fn allowed(from: usize, to: usize) -> bool {
        if (to as u64) <= HEAP[0].load(Relaxed) || (from as u64) >= HEAP[1].load(Relaxed) {
            return true;
        }
        ALLOWED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .range(..=from)
            .next_back()
            .is_some_and(|(_, entry)| entry.0 >= to)
    }

    /// A culling dispatch's bound was made for the buffer `heap` (start
    /// and end); `None`: it could not be.
    pub(crate) fn bound_made(heap: (usize, usize), ranges: Option<&[(usize, usize)]>) {
        HEAP[0].store(heap.0 as u64, Relaxed);
        HEAP[1].store(heap.1 as u64, Relaxed);
        match ranges {
            Some(ranges) => {
                COUNT[0].fetch_add(1, Relaxed);
                COUNT[2].fetch_add(ranges.len() as u64, Relaxed);
                COUNT[3].fetch_add(ranges.iter().map(|r| (r.1 - r.0) as u64).sum(), Relaxed);
                if super::mode() == 1 {
                    allow(ranges);
                }
            }
            None => {
                COUNT[1].fetch_add(1, Relaxed);
            }
        }
    }

    /// In a fetched piece shaders changed `bytes` bytes, `outside` of them
    /// outside every declared range (the first at `first`).
    pub(crate) fn changed(bytes: usize, outside: usize, first: Option<usize>) {
        COUNT[4].fetch_add(bytes as u64, Relaxed);
        COUNT[5].fetch_add(outside as u64, Relaxed);
        if let Some(first) = first {
            let _ = FIRST_OUTSIDE.compare_exchange(0, first as u64, Relaxed, Relaxed);
        }
    }

    /// With the frame line.
    pub(crate) fn line(frames: f64) {
        if super::mode() == 0 {
            return;
        }
        let k: Vec<f64> = COUNT
            .iter()
            .map(|c| c.swap(0, Relaxed) as f64 / frames)
            .collect();
        tracing::info!(
            "culling bounds per frame (VICTORY_CULL_RANGES): {:.1} made of {:.0} ranges ({:.2} MB), {:.1} could not be made",
            k[0],
            k[2],
            k[3] / 1e6,
            k[1],
        );
        if super::mode() != 1 {
            return;
        }
        let frame = crate::frame::current();
        ALLOWED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, entry| entry.1 + KEPT_FRAMES >= frame);
        tracing::info!(
            "culling bounds checked (VICTORY_CULL_RANGES=check): shaders changed {:.0} bytes a frame in the pieces fetched; of \
             those in the buffer, {:.0} lay outside every bound and every exactly declared range (first such address since the \
             start: {:#x})",
            k[4],
            k[5],
            FIRST_OUTSIDE.load(Relaxed),
        );
    }

    #[cfg(test)]
    mod tests {
        use super::{ALLOWED, HEAP, allow, allowed};
        use std::sync::atomic::Ordering::Relaxed;

        /// A byte range is allowed if one declared range, or declared
        /// ranges that touch, hold all of it, and anything outside the
        /// buffer is.
        #[test]
        fn declared_ranges_join_and_hold_what_lies_in_them() {
            ALLOWED.lock().unwrap_or_else(|e| e.into_inner()).clear();
            HEAP[0].store(0x1000, Relaxed);
            HEAP[1].store(0x9000, Relaxed);
            allow(&[(0x2000, 0x2100), (0x2200, 0x2300)]);
            assert!(allowed(0x2000, 0x2100) && allowed(0x2280, 0x2300));
            assert!(
                !allowed(0x20f0, 0x2110) && !allowed(0x2100, 0x2200) && !allowed(0x1ff0, 0x2010)
            );
            // The range between the two joins them; one over all three too.
            allow(&[(0x2100, 0x2200)]);
            assert!(allowed(0x2000, 0x2300) && allowed(0x20f0, 0x2210));
            allow(&[(0x1800, 0x2800)]);
            assert!(
                allowed(0x1800, 0x2800) && !allowed(0x17ff, 0x1801) && !allowed(0x27ff, 0x2801)
            );
            assert_eq!(ALLOWED.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
            // Outside the buffer nothing is checked.
            assert!(allowed(0x800, 0x1000) && allowed(0x9000, 0x9004) && !allowed(0x8ffc, 0x9004));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Batches, bound};

    /// A buffer of `len` bytes at a made-up title address, with dwords set.
    struct Heap(Vec<u8>);
    const AT: usize = 0x1000_0000;

    impl Heap {
        fn set(&mut self, at: usize, value: u32) {
            self.0[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        fn read(&self) -> impl Fn(usize, &mut [u8]) -> bool + '_ {
            |at, out| match at
                .checked_sub(AT)
                .and_then(|from| self.0.get(from..from + out.len()))
            {
                Some(bytes) => {
                    out.copy_from_slice(bytes);
                    true
                }
                None => false,
            }
        }
        /// A batch record at `b` with its model record at `m`.
        #[allow(clippy::too_many_arguments)]
        fn batch(
            &mut self,
            b: usize,
            m: usize,
            lists: [u32; 5],
            counters: u32,
            counts: u32,
            instances: u32,
            model: [u32; 8],
        ) {
            self.set(b + 8, m as u32);
            for (l, &list) in lists.iter().enumerate() {
                self.set(b + 12 + 4 * l, list);
            }
            self.set(b + 36, counters);
            self.set(b + 64, instances);
            self.set(b + 88, counts);
            for (i, &value) in model.iter().enumerate() {
                self.set(m + 104 + 4 * i, value);
            }
        }
    }

    /// The bound holds every address the programs can store to, for each
    /// batch that a group of the dispatch names, and nothing of a batch
    /// that none names.
    #[test]
    fn the_bound_holds_the_counters_the_counts_and_the_lists_of_each_batch() {
        let mut heap = Heap(vec![0; 0x10_0000]);
        // The table: groups 0 and 1 are of the batch at 0x100, group 2 of
        // the batch at 0x400. A third batch at 0x700 is in no group.
        let table = 0x40;
        for (g, b) in [0x100u32, 0x100, 0x400].into_iter().enumerate() {
            heap.set(table + 4 * g, b);
        }
        // Batch 1: records 10.., stride 4, ends up to 7; two lists; 100 instances.
        heap.batch(
            0x100,
            0x200,
            [0x2_0000, u32::MAX, 0x3_0000, u32::MAX, u32::MAX],
            0x8000,
            0xa000,
            100,
            [10, 1, 7, 3, 0, 0, 4, 2],
        );
        // Batch 2: records 0.., stride 0, no ends, tail 5; one list; 8 instances.
        heap.batch(
            0x400,
            0x500,
            [u32::MAX, u32::MAX, u32::MAX, u32::MAX, 0x4_0000],
            0x9000,
            0xb000,
            8,
            [0, 0, 0, 0, 0, 0, 0, 5],
        );
        heap.batch(
            0x700,
            0x800,
            [0x5_0000; 5],
            0xc000,
            0xd000,
            50,
            [0, 9, 9, 9, 9, 9, 9, 9],
        );
        let batches = Batches {
            heap: AT,
            heap_len: heap.0.len(),
            table: AT + table,
            groups: 3,
        };
        let ranges = bound(&batches, heap.read()).expect("records that can be read");
        // Batch 1 touches 3 * 4 + 7 = 19 records from 10; batch 2 touches 5 + 1 from 0.
        let want = [
            (0x8000 + 20 * 10, 0x8000 + 20 * 29),
            (0x9000, 0x9000 + 20 * 6),
            (0xa000 + 16 * 10, 0xa000 + 16 * 29),
            (0xb000, 0xb000 + 16 * 6),
            (0x2_0000, 0x2_0000 + 16 * 100),
            (0x3_0000, 0x3_0000 + 16 * 100),
            (0x4_0000, 0x4_0000 + 16 * 8),
        ];
        let want: Vec<(usize, usize)> = want.iter().map(|&(a, b)| (AT + a, AT + b)).collect();
        assert_eq!(ranges, want);
        // Every address of the three kinds lies in it: the counter of
        // record r (dword 1 of 20 bytes), its count, and the last slot of
        // the last pass of a list.
        let inside = |at: usize| {
            ranges
                .iter()
                .any(|&(a, b)| a <= AT + at && AT + at + 4 <= b)
        };
        for r in 10..29 {
            assert!(inside(0x8000 + 4 + 20 * r) && inside(0xa000 + 16 * r));
        }
        assert!(inside(0x2_0000 + 4 * (3 * 100 + 99)));
        assert!(!inside(0xc000 + 4) && !inside(0x5_0000));
    }

    /// Ranges with a little padding between them are one, and ranges
    /// further apart are not.
    #[test]
    fn ranges_a_few_bytes_apart_are_joined() {
        let mut heap = Heap(vec![0; 0x10_0000]);
        heap.set(0x40, 0x1000);
        // Lists of 64 bytes: the second 48 bytes behind the first, the
        // third 1 KiB behind the second.
        heap.batch(
            0x1000,
            0x1100,
            [
                0x4000,
                0x4000 + 64 + 48,
                0x4000 + 64 + 48 + 64 + 1024,
                u32::MAX,
                u32::MAX,
            ],
            0x2000,
            0x3000,
            4,
            [0, 1, 0, 0, 0, 0, 0, 0],
        );
        let batches = Batches {
            heap: AT,
            heap_len: heap.0.len(),
            table: AT + 0x40,
            groups: 1,
        };
        let ranges = bound(&batches, heap.read()).expect("records that can be read");
        assert_eq!(
            ranges,
            [
                (AT + 0x2000, AT + 0x2000 + 20),
                (AT + 0x3000, AT + 0x3000 + 16),
                (AT + 0x4000, AT + 0x4000 + 64 + 48 + 64),
                (
                    AT + 0x4000 + 64 + 48 + 64 + 1024,
                    AT + 0x4000 + 64 + 48 + 64 + 1024 + 64
                ),
            ]
        );
    }

    /// Ranges that touch or overlap are one; a batch named by many groups
    /// is read once.
    #[test]
    fn ranges_that_touch_are_joined() {
        let mut heap = Heap(vec![0; 0x10_0000]);
        for g in 0..64 {
            heap.set(0x40 + 4 * g, 0x1000);
        }
        // Counters and counts side by side, and two lists end to end.
        heap.batch(
            0x1000,
            0x1100,
            [0x4000, 0x4000 + 16 * 4, u32::MAX, u32::MAX, u32::MAX],
            0x2000,
            0x2000 + 20 * 2,
            4,
            [0, 2, 0, 0, 0, 0, 0, 0],
        );
        let batches = Batches {
            heap: AT,
            heap_len: heap.0.len(),
            table: AT + 0x40,
            groups: 64,
        };
        let ranges = bound(&batches, heap.read()).expect("records that can be read");
        assert_eq!(
            ranges,
            [
                (AT + 0x2000, AT + 0x2000 + 40 + 32),
                (AT + 0x4000, AT + 0x4000 + 16 * 8)
            ]
        );
    }

    /// A record that cannot be read, lies outside the buffer, or gives
    /// addresses outside it or sizes beyond belief has no bound: the whole
    /// view is declared then.
    #[test]
    fn records_that_do_not_fit_give_no_bound() {
        let fresh = || {
            let mut heap = Heap(vec![0; 0x10_0000]);
            heap.set(0x40, 0x100);
            heap.batch(
                0x100,
                0x200,
                [0x2_0000, u32::MAX, u32::MAX, u32::MAX, u32::MAX],
                0x8000,
                0xa000,
                16,
                [1, 2, 0, 0, 0, 0, 1, 0],
            );
            heap
        };
        let batches = |heap: &Heap| Batches {
            heap: AT,
            heap_len: heap.0.len(),
            table: AT + 0x40,
            groups: 1,
        };
        let heap = fresh();
        assert!(bound(&batches(&heap), heap.read()).is_some());
        // The table is not there.
        let mut gone = batches(&heap);
        gone.table = AT - 0x1000;
        assert!(bound(&gone, heap.read()).is_none());
        // The batch record lies past the buffer's end.
        let mut heap = fresh();
        heap.set(0x40, 0x10_0000 - 8);
        assert!(bound(&batches(&heap), heap.read()).is_none());
        // The model record does.
        let mut heap = fresh();
        heap.set(0x100 + 8, 0x10_0000 - 100);
        assert!(bound(&batches(&heap), heap.read()).is_none());
        // A list reaches past the end.
        let mut heap = fresh();
        heap.set(0x100 + 12, 0x10_0000 - 64);
        assert!(bound(&batches(&heap), heap.read()).is_none());
        // More records, or more instances, than a batch has.
        let mut heap = fresh();
        heap.set(0x200 + 104 + 28, u32::MAX);
        assert!(bound(&batches(&heap), heap.read()).is_none());
        let mut heap = fresh();
        heap.set(0x100 + 64, u32::MAX);
        assert!(bound(&batches(&heap), heap.read()).is_none());
    }
}
