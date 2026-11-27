//! Diagnostics (`VICTORY_BACK_PROBE=N`; off by default): whether the title
//! touches what shaders wrote before shaders write it again.
//!
//! What a submission's shaders wrote to host copies is put back into title
//! memory after it, 65 to 130 MB a frame in the world, because on the
//! console the card writes the title's memory itself and the title may
//! read it. Whether the title does read it decides whether those bytes
//! could stay on the card until they are asked for.
//!
//! With the switch, one in N of the 4 KiB pages that a put-back has just
//! written whole (private memory only: the title's heaps; not views, whose
//! protection the write guards own) is made inaccessible. The first access
//! to such a page faults into a handler of ours, which opens the page and
//! counts who it was: a read or a write by code that is not ours (the
//! title, or a library on its behalf), or code of ours outside the copy.
//! A copy of ours (a refresh reading the page, `direct_copy`) opens it
//! for the length of the copy and closes it again, so that it does not
//! count as the first touch. A page that is still closed when the next
//! put-back writes it was not touched in between: counted, with the frames
//! that went by.
//!
//! It slows the title where it does touch such pages (a fault each time)
//! and a system call that is handed such a page fails: a diagnostic for a
//! measured run, not a setting.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, NTSTATUS};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH,
    EXCEPTION_POINTERS,
};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_PRIVATE, MEMORY_BASIC_INFORMATION, PAGE_NOACCESS, PAGE_READWRITE,
    VirtualProtect, VirtualQuery,
};

const KERNEL_PAGE: usize = 4096;

/// Where the layer's own program is in memory (base, bytes); set at the
/// first use.
static LAYER: OnceLock<(usize, usize)> = OnceLock::new();

/// One page in this many is probed; 0: none.
pub(crate) fn every() -> u64 {
    static EVERY: OnceLock<u64> = OnceLock::new();
    *EVERY.get_or_init(|| {
        std::env::var("VICTORY_BACK_PROBE")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0)
    })
}

/// The pages made inaccessible, and the frame each was closed in.
static CLOSED: Mutex<BTreeMap<usize, u64>> = Mutex::new(BTreeMap::new());
/// The last pages a fault opened, each with the faults that may still
/// be tried again for it: another thread can stand at the same page.
static OPENED: Mutex<Vec<(usize, u8)>> = Mutex::new(Vec::new());
/// Since the last line: pages closed; first touched by a read that was
/// not ours, by a write that was not ours, by code of ours outside the
/// copy; written again by a put-back while still closed, and the
/// frames those had been closed for; opened for a copy of ours and
/// closed again.
static COUNT: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];

thread_local! {
    /// The pages this thread's running copy had opened for it.
    static AGAIN: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn protect(page: usize, protection: u32) {
    let mut old = 0;
    // SAFETY: a page of committed private memory of the title; only its
    // protection changes.
    unsafe { VirtualProtect(page as *const _, KERNEL_PAGE, protection, &mut old) };
}

/// A put-back is about to write title memory `[at, at + len)`: pages
/// of it that are still closed were not touched since the last one.
pub(crate) fn before(at: usize, len: usize) {
    if every() == 0 || len == 0 {
        return;
    }
    let frame = crate::frame::current();
    let mut closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    let pages: Vec<(usize, u64)> = closed
        .range(at & !(KERNEL_PAGE - 1)..at.saturating_add(len))
        .map(|(&page, &since)| (page, since))
        .collect();
    for (page, since) in pages {
        closed.remove(&page);
        protect(page, PAGE_READWRITE);
        COUNT[4].fetch_add(1, Relaxed);
        COUNT[5].fetch_add(frame.saturating_sub(since), Relaxed);
    }
}

/// A put-back has written title memory `[at, at + len)`: one in
/// [`every`] of the pages it wrote whole is closed.
pub(crate) fn after(at: usize, len: usize) {
    let every = every();
    if every == 0 {
        return;
    }
    install();
    let frame = crate::frame::current();
    let first = at.next_multiple_of(KERNEL_PAGE);
    let end = at.saturating_add(len) & !(KERNEL_PAGE - 1);
    for page in (first..end).step_by(KERNEL_PAGE) {
        // Another choice of pages in every frame.
        let pick = ((page / KERNEL_PAGE) as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(frame.wrapping_mul(0xD6E8_FEB8_6659_FD93))
            >> 20;
        if !pick.is_multiple_of(every) {
            continue;
        }
        // SAFETY: plain data filled in by the query.
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a valid out pointer of the right size.
        let answered =
            unsafe { VirtualQuery(page as *const _, &mut info, size_of_val(&info)) } != 0;
        if !answered
            || info.State != MEM_COMMIT
            || info.Type != MEM_PRIVATE
            || info.Protect != PAGE_READWRITE
        {
            continue;
        }
        let mut closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
        protect(page, PAGE_NOACCESS);
        closed.insert(page, frame);
        COUNT[0].fetch_add(1, Relaxed);
    }
}

/// A copy of ours has ended: the pages it had opened are closed again.
pub(super) fn closed_again() {
    let pages = AGAIN.with(|again| std::mem::take(&mut *again.borrow_mut()));
    if pages.is_empty() {
        return;
    }
    let closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    for page in pages {
        if closed.contains_key(&page) {
            protect(page, PAGE_NOACCESS);
        }
    }
}

fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Ahead of the copy's own handler, which takes a fault in the
        // copy for a page that is not there.
        super::direct_copy::install();
        // SAFETY: the headers of our own program's image, as loaded: the
        // offset of its PE header, and that header's size of the image.
        unsafe {
            let base = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null())
                as usize;
            let pe = base + ((base + 0x3c) as *const u32).read_unaligned() as usize;
            let size = ((pe + 24 + 56) as *const u32).read_unaligned() as usize;
            let _ = LAYER.set((base, size));
        }
        // SAFETY: the handler touches the probe's own state and the
        // protection of pages the probe closed.
        unsafe { AddVectoredExceptionHandler(1, Some(on_access)) };
    });
}

unsafe extern "system" fn on_access(info: *mut EXCEPTION_POINTERS) -> i32 {
    // SAFETY: the system passes the exception being dispatched.
    let (record, context) = unsafe { (&*(*info).ExceptionRecord, &*(*info).ContextRecord) };
    if record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION as NTSTATUS || record.NumberParameters < 2
    {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let page = record.ExceptionInformation[1] & !(KERNEL_PAGE - 1);
    let mut closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    if !closed.contains_key(&page) {
        // Opened a moment ago by another thread's fault: tried again,
        // a few times at most.
        let mut opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
        return match opened.iter_mut().find(|o| o.0 == page && o.1 > 0) {
            Some(o) => {
                o.1 -= 1;
                EXCEPTION_CONTINUE_EXECUTION
            }
            None => EXCEPTION_CONTINUE_SEARCH,
        };
    }
    if super::direct_copy::copying(context.Rip) {
        // A copy of ours: open for its length (`closed_again`).
        protect(page, PAGE_READWRITE);
        AGAIN.with(|again| again.borrow_mut().push(page));
        COUNT[6].fetch_add(1, Relaxed);
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    closed.remove(&page);
    protect(page, PAGE_READWRITE);
    {
        let mut opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
        if opened.len() >= 64 {
            opened.remove(0);
        }
        opened.push((page, 8));
    }
    let ours = LAYER
        .get()
        .is_some_and(|&(base, size)| (base..base + size).contains(&(context.Rip as usize)));
    const WRITE: usize = 1;
    let who = if ours {
        3
    } else if record.ExceptionInformation[0] == WRITE {
        2
    } else {
        1
    };
    COUNT[who].fetch_add(1, Relaxed);
    EXCEPTION_CONTINUE_EXECUTION
}

/// With the frame-rate line.
pub(super) fn line(frames: f64) {
    if every() == 0 {
        return;
    }
    let c: Vec<f64> = COUNT.iter().map(|c| c.swap(0, Relaxed) as f64).collect();
    let still = CLOSED.lock().unwrap_or_else(|e| e.into_inner()).len();
    tracing::info!(
        "probe of what shaders wrote (VICTORY_BACK_PROBE={}), per frame: {:.1} pages closed after a put-back; first touched \
         by a read that is not ours {:.1}, by a write that is not ours {:.1}, by code of ours outside the copy {:.1}; put \
         back again while still closed {:.1} (after {:.1} frames); opened for a copy of ours and closed again {:.1}; \
         {still} closed now",
        every(),
        c[0] / frames,
        c[1] / frames,
        c[2] / frames,
        c[3] / frames,
        c[4] / frames,
        if c[4] > 0.0 { c[5] / c[4] } else { 0.0 },
        c[6] / frames,
    );
}
