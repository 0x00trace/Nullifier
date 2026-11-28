//! Title memory by address, for translated shaders that read and write
//! through raw 40-bit addresses (`victory_shader::emit::SLOT_MEMORY`: the
//! console's ray-tracing structure builder and the programs that walk its
//! structures; the structures themselves lie in plain title memory, in no
//! buffer object).
//!
//! The copies are the ones every other access uses ([`crate::shadow`]: a
//! title buffer's copy, or the 4 MiB window of memory outside buffers), so
//! a root view and a raw address reach the same host bytes. This module
//! keeps what a shader needs to find them:
//!
//! - the **directory**: per 64 KiB page of title address space the number
//!   of the copy that holds the page (0: none) and the page's byte offset
//!   in it; one buffer for all lists, entries only ever go from empty to
//!   filled;
//! - per view heap a run of slots with a raw unordered access view of each
//!   numbered copy (slot 0 of the run: a null view, for pages without a
//!   copy);
//! - the **faults**: the page numbers shaders found empty, read back here
//!   once a frame, so the copies exist from the next frame on. A pointer a
//!   shader takes out of memory cannot be known before it runs; the
//!   addresses a dispatch is given as root views are made ready before it.
//!
//! **A page that is not there when a shader touches it is a lost access**:
//! the read gives 0, the store is dropped, and the fault note only helps
//! the next frame. For the structure builder that is the same as a dropped
//! stage, so before a dispatch the host makes ready everything it can see
//! the dispatch name: the whole copy around each root view, around each
//! pair of root constants that is a title address and around each root
//! constant that is one in 256-byte units (the builder passes structures
//! all three ways), and the 4 MiB after each (a structure may cross a
//! window's end).
//! shortcut: what a stage reaches only through pointers it reads out of
//! memory (InitHeaders' table of structure addresses, LoadInstances'
//! instance records) is not made ready unless something else named it;
//! the fault count in the log says whether that happens. The whole answer
//! is the extent of the title's allocation around an address, which the
//! memory module of the layer knows and this one does not ask yet.
//!
//! shortcut: a copy that is numbered is kept for the rest of the run, and
//! there are at most [`COPIES`] of them (4 MiB windows mostly: 4 GiB at
//! most). Every dispatch of such a shader brings all numbered copies up to
//! date and, if it can store, declares all of them written (the bytes are
//! compared on the CPU afterwards, as for any unordered access view): a
//! dirty-page mask written by the shader would bound both.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use victory_shader::emit::{MEMORY_FAULTS, MEMORY_PAGES};
use windows::Win32::Graphics::Direct3D12::*;

use crate::descriptor::host::HostHeap;
use crate::host::{HostList, host};
use crate::shadow::Shadow;

/// Copies that can be numbered (the run of heap slots is one longer). A
/// listed frame of the opening cutscene names about 50 windows.
const COPIES: u32 = 1023;
/// A window of memory outside buffers (`shadow.rs`).
const CELL: u64 = 4 << 20;
/// A directory page.
const PAGE: u64 = 1 << 16;

struct Memory {
    /// Buffer, its mapping, its GPU address.
    directory: (ID3D12Resource, *mut u32, u64),
    faults: (ID3D12Resource, *mut u32, u64),
    /// Copy `n` is `copies[n - 1]`.
    copies: Vec<Arc<Shadow>>,
    /// What each copy was named for, as `copies`.
    named: Vec<Named>,
    /// The copies that came to be [`Named::Whole`] in the [`ready`] under
    /// way: they are brought up to date like copies numbered in it.
    grown: Vec<usize>,
    /// Per host view heap (by address): the first slot of its run, and how
    /// many copies have their view there.
    heaps: HashMap<usize, (u32, u32)>,
    /// The frame the faults were last read in.
    frame: u64,
    /// Pages asked for this frame that are not the title's memory.
    refused: std::collections::HashSet<u64>,
}

/// What a numbered copy is in the directory for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Named {
    /// Only descriptor tables in it were named, for shaders that read a
    /// table descriptor's dwords (`USER_TABLE_ADDRESS`). Only the tables'
    /// pages have entries and are brought up to date, and nothing of the
    /// copy is declared written.
    Tables,
    /// A root view, a root constant or a shader's own pointer named it:
    /// every page has its entry, and each dispatch brings all of it up to
    /// date and, if it can store, declares all of it written.
    Whole,
}

/// What a copy is named for once `table` (a descriptor table) or something
/// else names it, having been named for `was` before; and whether all its
/// pages get their entries now.
fn named_now(was: Option<Named>, table: bool) -> (Named, bool) {
    match (was, table) {
        (Some(Named::Whole), _) => (Named::Whole, false),
        (_, false) => (Named::Whole, true),
        (_, true) => (Named::Tables, false),
    }
}

/// The two directory pages a descriptor table at `address` is named by:
/// the one it starts in and the next (a table may cross a page's end).
fn table_pages(address: u64) -> [u64; 2] {
    let first = address & !(PAGE - 1);
    [first, first + PAGE]
}

// SAFETY: the mappings are of buffers the struct owns; all access is under
// the mutex.
unsafe impl Send for Memory {}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

/// What a draw binds.
pub(crate) struct Ready {
    pub(crate) directory: u64,
    pub(crate) faults: u64,
    /// The view heap slot of the null view; copy `n`'s view is `n` further.
    pub(crate) base: u32,
}

impl Memory {
    fn new() -> Option<Memory> {
        let host = host()?;
        let make = |dwords: u64, what: &str| {
            let (buffer, data) = host.shared_buffer(dwords * 4)?;
            crate::host::name(&buffer, || format!("title memory by address: {what}"));
            // SAFETY: a fresh mapping of at least that many bytes.
            unsafe { std::ptr::write_bytes(data, 0, (dwords * 4) as usize) };
            // SAFETY: a live buffer.
            let gpu = unsafe { buffer.GetGPUVirtualAddress() };
            Some((buffer, data.cast::<u32>(), gpu))
        };
        tracing::info!("title memory by address: a shader reads or writes through raw addresses");
        Some(Memory {
            directory: make(u64::from(MEMORY_PAGES) * 2, "directory")?,
            faults: make(u64::from(MEMORY_FAULTS), "faults")?,
            copies: Vec::new(),
            named: Vec::new(),
            grown: Vec::new(),
            heaps: HashMap::new(),
            frame: u64::MAX,
            refused: Default::default(),
        })
    }

    /// Gives every page that copy `n` holds whole its entry, if it has
    /// none: a structure is more than 64 KiB.
    /// shortcut: a page of a title buffer inside this one that has a copy of
    /// its own is given this copy too.
    fn enter_all(&mut self, n: usize) {
        let (first, len) = self.copies[n].span();
        let mut at = first.next_multiple_of(PAGE);
        while at + PAGE <= first + len && at / PAGE < u64::from(MEMORY_PAGES) - 1 {
            // SAFETY: inside the directory's mapping.
            let other = unsafe { self.directory.1.add((at / PAGE) as usize * 2) };
            // SAFETY: as above; the offset first, as in `map`.
            unsafe {
                if other.read() == 0 {
                    other.add(1).write_volatile((at - first) as u32);
                    other.write_volatile(n as u32 + 1);
                }
            }
            at += PAGE;
        }
    }

    /// Copy `n` is named again (`table`: by a descriptor table): if it was
    /// there for tables only and something else names it now, it is there
    /// whole from here on.
    fn name_again(&mut self, n: usize, table: bool) {
        let (now, all) = named_now(Some(self.named[n]), table);
        self.named[n] = now;
        if all {
            self.enter_all(n);
            self.grown.push(n);
        }
    }

    /// Gives the page of `address` its copy, if it has none and the memory
    /// is the title's. `table`: the address is a descriptor table's, named
    /// for its descriptors' dwords only ([`Named::Tables`]).
    fn map(&mut self, address: u64, table: bool) {
        let page = address / PAGE;
        // The last entry stays empty: addresses past the directory read it.
        if page >= u64::from(MEMORY_PAGES) - 1 {
            return;
        }
        // SAFETY: inside the directory's mapping.
        let entry = unsafe { self.directory.1.add(page as usize * 2) };
        // SAFETY: as above.
        let held = unsafe { entry.read() };
        if held != 0 {
            self.name_again(held as usize - 1, table);
            return;
        }
        let start = page * PAGE;
        // A shader's pointer, or a constant taken for an address, may be
        // anything: only memory the title has.
        if self.refused.contains(&page) {
            return;
        }
        if !crate::texture::committed(start as *const u8, PAGE as usize) {
            self.refused.insert(page);
            return;
        }
        // The copy a root view at that address would get; a buffer that
        // ends inside the page is passed over for what holds all of it.
        let whole = |s: &Arc<Shadow>| s.offset_in(start).is_some_and(|_| s.left(start) >= PAGE);
        let Some(shadow) = crate::shadow::find(start, 1)
            .filter(whole)
            .or_else(|| crate::shadow::find(start, PAGE).filter(whole))
        else {
            return;
        };
        // SAFETY: a live buffer.
        let flags = unsafe { shadow.buffer.GetDesc() }.Flags;
        if flags.0 & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS.0 == 0 {
            return;
        }
        let n = match self.copies.iter().position(|c| Arc::ptr_eq(c, &shadow)) {
            Some(n) => {
                self.name_again(n, table);
                n
            }
            None if self.copies.len() < COPIES as usize => {
                self.copies.push(shadow.clone());
                let n = self.copies.len() - 1;
                let (named, all) = named_now(None, table);
                self.named.push(named);
                // Every page the copy holds whole, not only the one asked
                // for, unless it is a table's: the title's tables lie in
                // its largest buffer, and with all of that buffer in the
                // directory every dispatch brought 174 MiB up to date and
                // declared it written (docs/FINDINGS.md 2.28).
                if all {
                    self.enter_all(n);
                }
                n
            }
            None => {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    tracing::warn!(COPIES, "title memory by address: no number left for another copy; its pages read zeros")
                });
                return;
            }
        };
        let offset = shadow.offset_in(start).unwrap_or(0);
        // The offset first: a shader that sees the number sees it too.
        // SAFETY: inside the directory's mapping.
        unsafe {
            entry.add(1).write_volatile(offset as u32);
            entry.write_volatile(n as u32 + 1);
        }
    }

    /// The pages shaders found empty since the last frame.
    fn take_faults(&mut self) {
        let mut pages = Vec::new();
        for i in 1..MEMORY_FAULTS as usize {
            // SAFETY: inside the faults' mapping; the GPU may be writing,
            // a number missed now is written again next frame.
            let at = unsafe { self.faults.1.add(i) };
            // SAFETY: as above.
            let page = unsafe { at.read_volatile() };
            if page != 0 {
                pages.push(page);
                // SAFETY: as above.
                unsafe { at.write_volatile(0) };
            }
        }
        if !pages.is_empty() {
            // Each is an access that read 0 or was dropped.
            static SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            if SAID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 50 {
                tracing::warn!(
                    pages = pages.len(),
                    first = format_args!("{:#x}", u64::from(pages[0]) * PAGE),
                    "title memory by address: shaders touched pages that had no copy (those accesses read 0 or were dropped); the copies are made now"
                );
            }
        }
        for page in pages {
            self.map(u64::from(page) * PAGE, false);
        }
    }

    /// What a dispatch names: the copy around `address` and the 4 MiB
    /// after it.
    fn name(&mut self, address: u64) {
        self.map(address, false);
        self.map((address & !(CELL - 1)) + CELL, false);
    }

    /// The copy that the page at `address` has its entry for.
    fn copy_of(&self, address: u64) -> Option<usize> {
        let page = address / PAGE;
        if page >= u64::from(MEMORY_PAGES) - 1 {
            return None;
        }
        // SAFETY: inside the directory's mapping.
        let held = unsafe { self.directory.1.add(page as usize * 2).read() };
        (held != 0).then(|| held as usize - 1)
    }
}

/// Before a draw whose shader reads title memory by address: what its
/// root arguments name gets its copies (`addresses`: its root views;
/// `tables`: its descriptor tables; `constants`: its root constants, see
/// the module). Every numbered copy has its view in `heap`'s run. A copy
/// named whole is brought up to date when `list` is submitted (and, with
/// `writes`, copied back after); of a copy named for tables only, the
/// tables' pages are. None if the host cannot hold it; the draw is then
/// dropped.
pub(crate) fn ready(
    list: &HostList,
    heap: &HostHeap,
    addresses: &[u64],
    tables: &[u64],
    constants: &[u32],
    writes: bool,
) -> Option<Ready> {
    let mut guard = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Memory::new();
    }
    let m = guard.as_mut()?;
    let frame = crate::frame::current();
    if m.frame != frame {
        m.frame = frame;
        m.refused.clear();
        m.take_faults();
    }
    let numbered = m.copies.len();
    m.grown.clear();
    for &address in addresses {
        m.name(address);
    }
    // A table is named for its descriptors' dwords: the page it starts in
    // and the next.
    // shortcut: a descriptor further into a table than that reads zeros
    // once; its page is then named by the fault, and the copy is whole from
    // the next frame on.
    for &table in tables {
        for page in table_pages(table) {
            m.map(page, true);
        }
    }
    // A title address in two constants (low dword, then bits 32 to 39), or
    // in one in 256-byte units. A constant that only looks like one names
    // memory nothing reads; one that is not the title's memory is refused.
    for (i, &c) in constants.iter().enumerate() {
        if let Some(&high) = constants.get(i + 1)
            && (1..=0xFF).contains(&high)
            && c >= 0x1_0000
        {
            m.name(u64::from(c) | u64::from(high) << 32);
        }
        if c >= 0x0100_0000 {
            m.name(u64::from(c) << 8);
        }
    }
    let key = std::ptr::from_ref(heap) as usize;
    let (base, made) = match m.heaps.get(&key) {
        Some(&run) => run,
        None => {
            // Held for the heap's life: not given back with a list's slots.
            let base = heap.dynamic(COPIES + 1)?;
            heap.null_raw_uav_at(base);
            (base, 0)
        }
    };
    for (n, shadow) in m.copies.iter().enumerate().skip(made as usize) {
        let (_, len) = shadow.span();
        heap.raw_uav_at(base + 1 + n as u32, &shadow.buffer, 0, len);
    }
    m.heaps.insert(key, (base, m.copies.len() as u32));
    // Once per list, and for copies numbered since: the builder alone
    // dispatches hundreds of times a frame.
    // shortcut: a copy another list numbers meanwhile is brought up to date
    // by that list's submission, which may run after this one's.
    const READ: usize = usize::MAX - 0x6d;
    const WRITE: usize = usize::MAX - 0x6e;
    // Below these: a table's page, once a list.
    const TABLE: usize = usize::MAX - 0x1_0000_0000;
    let first_read = list.first_use(READ);
    let first_write = writes && list.first_use(WRITE);
    for (n, shadow) in m.copies.iter().enumerate() {
        if m.named[n] == Named::Tables {
            continue;
        }
        let new = n >= numbered || m.grown.contains(&n);
        let (start, len) = shadow.span();
        if first_read || new {
            shadow.refresh(list, start, len);
        }
        if writes && (first_write || new) {
            shadow.gpu_writes(list, start, len);
        }
    }
    // Of a copy that is there for tables only: the tables' pages, read by
    // the shader as the title has them now. No shader stores to them.
    for &table in tables {
        for page in table_pages(table) {
            if let Some(n) = m.copy_of(page)
                && m.named[n] == Named::Tables
                && list.first_use(TABLE - (page / PAGE) as usize)
            {
                m.copies[n].refresh(list, page, PAGE);
            }
        }
    }
    if first_read {
        list.keep(&m.directory.0);
        list.keep(&m.faults.0);
    }
    Some(Ready {
        directory: m.directory.2,
        faults: m.faults.2,
        base,
    })
}

#[cfg(test)]
mod tests {
    use super::{Named, PAGE, named_now, table_pages};

    /// A copy that only descriptor tables name is there for its tables; a
    /// root view, a constant or a shader's pointer makes it whole, once, and
    /// a table named afterwards leaves it whole.
    #[test]
    fn a_copy_named_by_tables_only_is_not_there_whole() {
        // Numbered now: by a table, by anything else.
        assert_eq!(named_now(None, true), (Named::Tables, false));
        assert_eq!(named_now(None, false), (Named::Whole, true));
        // Named again.
        assert_eq!(named_now(Some(Named::Tables), true), (Named::Tables, false));
        assert_eq!(named_now(Some(Named::Tables), false), (Named::Whole, true));
        assert_eq!(named_now(Some(Named::Whole), true), (Named::Whole, false));
        assert_eq!(named_now(Some(Named::Whole), false), (Named::Whole, false));
    }

    /// A table is named by the page it starts in and the next one.
    #[test]
    fn a_table_is_named_by_two_pages() {
        assert_eq!(table_pages(5 * PAGE), [5 * PAGE, 6 * PAGE]);
        assert_eq!(table_pages(5 * PAGE + 0x1720), [5 * PAGE, 6 * PAGE]);
        assert_eq!(table_pages(6 * PAGE - 4), [5 * PAGE, 6 * PAGE]);
    }
}
