//! The title's output size (`VICTORY_OUTPUT`), ported from
//! Belobug/VIctory-Belobug-AMD `crates/victory-loader/src/resolution.rs`
//! (GPL-3.0-or-later, same licence). The byte sites are the title's own,
//! compared before anything is written; another build is left alone.
//!
//! The title applies its settings' fullscreen size through `+0x22434a0`
//! and then presents at that size whatever `settings.xml` says: the console
//! profile wins. This replaces that block so the size asked for is written
//! into the live settings and handed to the title's own screen-size setter.
//! The back-buffer pool's left-over check is skipped for a size other than
//! 3840x2160.

/// Sizes the title has a display mode for (its own list, +0x615c0c0).
pub const TITLE_MODES: [(u32, u32); 5] = [
    (1280, 720),
    (1664, 936),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// Sizes `VICTORY_OUTPUT` accepts: the title's own five modes, then the 21:9
/// and 32:9 sizes it has no mode for but whose setter takes them (the same
/// one the modes go through). The render and the host swap chain use whatever
/// this says; the title's UI is designed for 16:9, which is what the captures
/// at 3440x1440 are checked for.
pub const MODES: [(u32, u32); 9] = [
    (1280, 720),
    (1664, 936),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
    (2560, 1080),
    (3440, 1440),
    (3840, 1080),
    (5120, 1440),
];

/// What happened to a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Patched {
    Done,
    Differs,
    Failed(&'static str),
}

/// `VICTORY_OUTPUT=<w>x<h>`: one of [`MODES`]. `None` if unset or not one
/// (said why).
pub fn asked_output() -> Option<(u32, u32)> {
    let spec = std::env::var("VICTORY_OUTPUT").ok()?;
    let size = parse_output(&spec);
    if size.is_none() {
        eprintln!(
            "Nullifier: VICTORY_OUTPUT={spec}: expected 1920x1080, 2560x1440, 1280x720, 1664x936, 3840x2160, 2560x1080, 3440x1440, 3840x1080 or 5120x1440"
        );
    }
    size
}

pub fn parse_output(spec: &str) -> Option<(u32, u32)> {
    let (w, h) = spec.trim().split_once(['x', 'X'])?;
    let size = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    MODES.contains(&size).then_some(size)
}

/// `VICTORY_RENDER_SCALE=<s>`, 0.5 to 1.0: the title's own upscaler's scale
/// (2/3 without it). At 1.0 the title renders at the output size.
pub fn asked_render_scale() -> Option<f32> {
    let spec = std::env::var("VICTORY_RENDER_SCALE").ok()?;
    let scale = parse_render_scale(&spec);
    if scale.is_none() {
        eprintln!("Nullifier: VICTORY_RENDER_SCALE={spec}: expected 0.5 to 1.0");
    }
    scale
}

pub fn parse_render_scale(spec: &str) -> Option<f32> {
    spec.trim()
        .parse::<f32>()
        .ok()
        .filter(|scale| (0.5..=1.0).contains(scale))
}

/// A place to change: its offset in the image and the bytes there.
pub struct Site {
    pub at: usize,
    pub what: &'static str,
    pub bytes: &'static [u8],
}

pub const SET_SCREEN_SIZE: usize = 0x2478130;
pub const SCREEN_HEIGHT: usize = 0xa9438d0;
pub const HEIGHT_CHANGED: usize = 0xd8c3a0;

/// +0x22434a0 from +0x224352b to +0x2243568.
pub const OUTPUT: Site = Site {
    at: 0x224352b,
    what: "settings' output size applied",
    bytes: &[
        0x8b, 0x0d, 0x9b, 0x03, 0x70, 0x08, // mov ecx, [+0xa9438cc]
        0x8b, 0x86, 0x04, 0x02, 0x00, 0x00, // mov eax, [rsi+204h]
        0x39, 0xc1, 0x75, 0x0d, // cmp ecx, eax ; jne
        0x8b, 0x96, 0x08, 0x02, 0x00, 0x00, // mov edx, [rsi+208h]
        0xe8, 0xea, 0x4b, 0x23, 0x00, 0xeb, 0x20, // call +0x2478130 ; jmp +0x2243568
        0x8b, 0xbe, 0x08, 0x02, 0x00, 0x00, // mov edi, [rsi+208h]
        0x8b, 0x2d, 0x7c, 0x03, 0x70, 0x08, // mov ebp, [+0xa9438d0]
        0x89, 0xc1, 0x89, 0xfa, // mov ecx, eax ; mov edx, edi
        0xe8, 0xd3, 0x4b, 0x23, 0x00, // call +0x2478130
        0x39, 0xfd, 0x74, 0x07, 0xb1, 0x01, // cmp ebp, edi ; je ; mov cl, 1
        0xe8, 0x38, 0x8e, 0xb4, 0xfe, // call +0xd8c3a0
    ],
};

/// +0x247ad10's left-over check: `cmp rax, 1FFFFFh ; jbe +1 ; int3`.
pub const POOL_CHECK: Site = Site {
    at: 0x247af25,
    what: "back buffer pool's left-over check",
    bytes: &[0x48, 0x3d, 0xff, 0xff, 0x1f, 0x00, 0x76, 0x01, 0xcc],
};

/// The title's own upscaler's scale, `mov dword [rcx+90h], 2/3`.
pub const OWN_SCALE: Site = Site {
    at: 0x28f1e78,
    what: "the title's own upscaler's render scale",
    bytes: &[0xc7, 0x81, 0x90, 0x00, 0x00, 0x00, 0xab, 0xaa, 0x2a, 0x3f],
};

/// The last of the title's five modes (3840x2160@60), at +0x615c0c0 + 4 * 16:
/// `(width, height, refresh numerator, denominator)` per entry. A size the
/// title has no mode for replaces this one, so its own selection can pick it:
/// on a display narrower than 3840 it otherwise clamps to 2560x1440.
pub const LARGEST_MODE: Site = Site {
    at: 0x615c100,
    what: "the title's largest display mode",
    bytes: &[
        0x00, 0x0f, 0x00, 0x00, // 3840
        0x70, 0x08, 0x00, 0x00, // 2160
        0x3c, 0x00, 0x00, 0x00, // 60
        0x01, 0x00, 0x00, 0x00, // 1
    ],
};

/// What goes at [`LARGEST_MODE`] for `width` x `height`.
pub fn largest_mode_code(width: u32, height: u32) -> Vec<u8> {
    let mut c = Vec::with_capacity(LARGEST_MODE.bytes.len());
    c.extend(width.to_le_bytes());
    c.extend(height.to_le_bytes());
    c.extend([0x3c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);
    c
}

/// What goes at [`OWN_SCALE`] for `scale`.
pub fn own_scale_code(scale: f32) -> Vec<u8> {
    let mut c = OWN_SCALE.bytes[..6].to_vec();
    c.extend(scale.to_le_bytes());
    c
}

pub fn pool_check_code() -> Vec<u8> {
    let mut c = POOL_CHECK.bytes[..8].to_vec();
    c[6] = 0xeb;
    c
}

fn rel(to: usize, next: usize) -> [u8; 4] {
    ((to as i64 - next as i64) as i32).to_le_bytes()
}

/// What goes at [`OUTPUT`] for `width` x `height`.
pub fn output_code(width: u32, height: u32) -> Vec<u8> {
    let at = OUTPUT.at;
    let mut c = Vec::with_capacity(OUTPUT.bytes.len());
    c.push(0xb8); // mov eax, width
    c.extend(width.to_le_bytes());
    c.extend([0x89, 0x86, 0x04, 0x02, 0x00, 0x00]); // mov [rsi+204h], eax
    c.push(0xbf); // mov edi, height
    c.extend(height.to_le_bytes());
    c.extend([0x89, 0xbe, 0x08, 0x02, 0x00, 0x00]); // mov [rsi+208h], edi
    c.extend([0x8b, 0x2d]); // mov ebp, [screen height]
    c.extend(rel(SCREEN_HEIGHT, at + c.len() + 4));
    c.extend([0x89, 0xc1, 0x89, 0xfa]); // mov ecx, eax ; mov edx, edi
    c.push(0xe8);
    c.extend(rel(SET_SCREEN_SIZE, at + c.len() + 4));
    let end = OUTPUT.bytes.len();
    c.extend([0x39, 0xfd, 0x74, (end - (c.len() + 4)) as u8]); // cmp ebp, edi ; je end
    c.extend([0xb1, 0x01, 0xe8]);
    c.extend(rel(HEIGHT_CHANGED, at + c.len() + 4));
    c.extend([0xeb, (end - (c.len() + 2)) as u8]);
    c.resize(end, 0xcc);
    c
}

/// Compares `site`'s bytes in the image of `size` bytes at `base`.
///
/// # Safety
/// The image is mapped.
pub unsafe fn matches(base: usize, size: usize, site: &Site) -> Patched {
    if site
        .at
        .checked_add(site.bytes.len())
        .is_none_or(|end| end > size)
    {
        return Patched::Failed("outside the image");
    }
    // SAFETY: inside the mapped image (checked above).
    let there =
        unsafe { std::slice::from_raw_parts((base + site.at) as *const u8, site.bytes.len()) };
    if there == site.bytes {
        Patched::Done
    } else {
        Patched::Differs
    }
}

/// Writes `code` over `site` if its bytes are the expected ones.
///
/// # Safety
/// The image stays mapped and no thread runs its code yet.
#[cfg(windows)]
pub unsafe fn write(base: usize, size: usize, site: &Site, code: &[u8]) -> Patched {
    use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
    use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    if code.len() > site.bytes.len() {
        return Patched::Failed("replacement longer than the bytes compared");
    }
    // SAFETY: the caller's promise.
    let found = unsafe { matches(base, size, site) };
    if found != Patched::Done {
        return found;
    }
    let at = base + site.at;
    let mut old = 0;
    // SAFETY: inside the image (checked); made writable for this and put
    // back; nothing runs it yet.
    unsafe {
        if VirtualProtect(at as *const _, code.len(), PAGE_EXECUTE_READWRITE, &mut old) == 0 {
            return Patched::Failed("Windows would not make it writable");
        }
        std::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, code.len());
        VirtualProtect(at as *const _, code.len(), old, &mut old);
        FlushInstructionCache(GetCurrentProcess(), at as *const _, code.len());
    }
    Patched::Done
}

/// Applies `VICTORY_OUTPUT`, if asked for; a line each in the log and on
/// standard output.
///
/// # Safety
/// The image of `size` bytes at `base` stays mapped and no thread runs its
/// code yet.
#[cfg(windows)]
pub unsafe fn apply_asked(base: usize, size: usize) {
    let mut sites: Vec<(&Site, Vec<u8>, String)> = Vec::new();
    if let Some((w, h)) = asked_output() {
        if !TITLE_MODES.contains(&(w, h)) {
            sites.push((&LARGEST_MODE, largest_mode_code(w, h), format!("{w}x{h}")));
        }
        sites.push((&OUTPUT, output_code(w, h), format!("{w}x{h}")));
        if (w, h) != (3840, 2160) {
            sites.push((&POOL_CHECK, pool_check_code(), format!("{w}x{h}")));
        }
    }
    if let Some(scale) = asked_render_scale() {
        sites.push((&OWN_SCALE, own_scale_code(scale), format!("{scale}")));
    }
    if sites.is_empty() {
        return;
    }
    for (site, code, value) in sites {
        // SAFETY: the caller's promise.
        let done = unsafe { write(base, size, site, &code) };
        let offset = format!("{:#x}", site.at);
        match done {
            Patched::Done => tracing::info!(site = %offset, what = site.what, value, "resolution: patched"),
            Patched::Differs => tracing::warn!(site = %offset, what = site.what, "resolution: not patched, the title's bytes there are not the ones expected (another build?)"),
            Patched::Failed(why) => tracing::warn!(site = %offset, what = site.what, why, "resolution: not patched"),
        }
        println!("resolution: {} ({value}) at title+{offset}: {done:?}", site.what);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_parse() {
        assert_eq!(parse_output("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_output(" 2560X1440 "), Some((2560, 1440)));
        assert_eq!(parse_output("1600x900"), None);
        assert_eq!(parse_output("1080p"), None);
    }

    #[test]
    fn the_ultrawide_sizes_parse() {
        for size in [(2560, 1080), (3440, 1440), (3840, 1080), (5120, 1440)] {
            let spec = format!("{}x{}", size.0, size.1);
            assert_eq!(parse_output(&spec), Some(size), "{spec}");
            assert!(MODES.contains(&size));
        }
        assert!(
            MODES
                .iter()
                .all(|s| TITLE_MODES.contains(s) || s.0 * 9 != s.1 * 16)
        );
    }

    #[test]
    fn the_output_code_keeps_the_titles_else_path() {
        let c = output_code(1920, 1080);
        assert_eq!(c.len(), OUTPUT.bytes.len());
        assert_eq!(c[0], 0xb8);
        assert_eq!(c[1..5], 1920u32.to_le_bytes());
        assert_eq!(c[12..16], 1080u32.to_le_bytes());
        let target = |op_end: usize| {
            (OUTPUT.at + op_end) as i64
                + i64::from(i32::from_le_bytes(c[op_end - 4..op_end].try_into().unwrap()))
        };
        assert_eq!(target(28), SCREEN_HEIGHT as i64);
        assert_eq!(target(37), SET_SCREEN_SIZE as i64);
        assert_eq!(41 + c[40] as usize, 61, "je lands at the end");
        assert_eq!(target(48), HEIGHT_CHANGED as i64);
        assert_eq!(50 + c[49] as usize, 61, "jmp lands at the end");
        assert!(c[50..].iter().all(|&x| x == 0xcc));
    }
}

#[cfg(test)]
mod largest_mode_tests {
    use super::*;

    #[test]
    fn the_largest_mode_code_writes_the_size_at_sixty() {
        let c = largest_mode_code(3440, 1440);
        assert_eq!(c.len(), LARGEST_MODE.bytes.len());
        assert_eq!(&c[0..4], &3440u32.to_le_bytes());
        assert_eq!(&c[4..8], &1440u32.to_le_bytes());
        assert_eq!(&c[8..], &[0x3c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);
        let stock = largest_mode_code(3840, 2160);
        assert_eq!(
            stock, LARGEST_MODE.bytes,
            "3840x2160 is the bytes already there"
        );
    }
}
