//! Indirect execution's arguments, bounded before the GPU reads them. The
//! title's own compute passes write most of them; a pass we drop or
//! translate wrongly leaves whatever the memory held, and a draw of four
//! billion vertices or a dispatch past the host's limits runs until the
//! host resets the display driver. So active records are copied and clamped
//! on the GPU into a device-local scratch buffer, just before execution, by a
//! compute shader of ours (HLSL compiled by the system's compiler; a 5.1
//! shader with its root signature inside), leaving the title's argument and
//! count buffers untouched.
//!
//! The scratch buffer is left readable as indirect arguments after a clamp
//! and stays so until the next clamp into it, which moves it straight back
//! to unordered access; it goes back to the common state once, when the
//! list closes. The executions that read it record no barrier of their
//! own, so draws that read records clamped together (`crate::draw`'s
//! windows) follow each other with nothing between them. Before, each
//! execution was followed by a move of the scratch buffer back to the
//! common state, which drivers may treat as a full wait.
//!
//! A run of single records ([`bound_run`]) is clamped by one pass in front
//! of the first of its draws: the pass reads a table that names where each
//! record lies, and the table is filled as the draws after it are recorded,
//! up to the next command that may write arguments. The title records its
//! mesh draws in such runs, some hundreds long, each record somewhere else
//! in one large argument buffer.
//!
//! When a count buffer is given, threads at or past `min(max, count)` return
//! immediately without charging the list's budget or reading uninitialized
//! tail slots. Each list has a budget (2^27 vertex instances or threads,
//! groups counted as 64 threads, mesh groups as 128): active records past it
//! run empty, saturating without 32-bit wraparound so a list of thousands of
//! garbage records cannot run for seconds either.
//!
//! shortcut: the bounds (2^24 vertices, 2^26 vertex instances, 65,535
//! groups per dimension and 2^20 per dispatch, one-dimensional mesh
//! dispatches, the budget) cut legitimate work beyond them; raise them if
//! the title needs more.

use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

use crate::host::{HostList, host, transition};

// Written for both compilers `D3DCompile` may be: Microsoft's and, under
// Wine, vkd3d-shader's, which lacks `ByteAddressBuffer`/`RWByteAddressBuffer`
// methods, `[allow_uav_condition]`, and embedded `[RootSignature(...)]`.
// Root SRVs/UAVs take structured word buffers identically.
const SOURCE: &str = r#"
cbuffer Record : register(b0) {
    uint kind;
    uint stride;
    uint records;
    uint args_offset;
    // Bit 0: a count buffer bounds the records. Bit 1: the records are a
    // window clamped ahead of their draws: one that asks for more than can
    // run is left empty and costs nothing. Bit 2: one dispatch, written as
    // SPLIT records of at most 65,535 groups in x each. Bit 3, with bit 0:
    // the records are a run found by a table (`bound_run`): behind the
    // count, word i of `count_buf` is the byte offset in the source where
    // record i lies; it goes to slot i.
    uint has_count;
    uint count_offset;
};
StructuredBuffer<uint> src_args : register(t0);
StructuredBuffer<uint> count_buf : register(t1);
RWStructuredBuffer<uint> dst_args : register(u0);
// The list's work so far (vertex instances, threads): past the budget,
// records run empty.
RWStructuredBuffer<uint> budget : register(u1);
static const uint BUDGET = 1u << 27;
// The vertices or indices an instance from which a draw record runs empty.
static const uint MOST_DRAW = 1u << 23;
// Groups of one dispatch; more run empty.
static const uint GROUPS = 1u << 20;
static const uint SPLIT = 2;
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint eff_count = records;
    bool ahead = (has_count & 2u) != 0;
    bool split = (has_count & 4u) != 0;
    if ((has_count & 1u) != 0) {
        eff_count = min(eff_count, count_buf[count_offset / 4u]);
    }
    if (id.x >= eff_count) return;
    uint stride_words = stride / 4u;
    uint src_at = (args_offset / 4u) + id.x * stride_words;
    // (A branch, not a select: without the bit the view behind `count_buf`
    // is a count of four bytes, or the source itself, and is not read here.)
    [branch]
    if ((has_count & 8u) != 0) {
        src_at = count_buf[(count_offset / 4u) + 1u + id.x] / 4u;
    }
    uint dst_at = id.x * stride_words;
    uint cost;
    uint2 draw_counts = uint2(0, 0);
    uint3 groups = uint3(0, 0, 0);
    if (kind <= 1) {
        // Draws: vertex or index count, then instances.
        uint2 asked = uint2(src_args[src_at], src_args[src_at + 1u]);
        if (ahead && (asked.x > (1u << 24) || asked.y > (1u << 24))) {
            asked = uint2(0, 0);
        }
        uint count = min(asked.x, 1u << 24);
        uint instances = min(asked.y, (1u << 26) / max(count, 1u));
        cost = count * instances;
        // A record that asks for MOST_DRAW vertices or indices an instance
        // or more is no draw of the title's (no mesh of a console title has
        // 2.8 million triangles): it runs empty and charges nothing. The
        // tree this rule comes from (fxceless/victory-fork f85101b) found
        // one list with 90 draws of 2^24 vertices and one instance each.
        // Clamped one draw at a time, the list's budget lets eight of them
        // run (2^27 in all) and empties the other 82; served from a window,
        // whose records are charged once however often they are drawn, all
        // 90 ran: 1.5 thousand million vertices in one list, with windows
        // on in the runs that lost the device there.
        if (count >= MOST_DRAW) {
            count = 0;
            instances = 0;
            cost = 0;
        }
        draw_counts = uint2(count, instances);
    } else if (kind == 2) {
        uint3 asked = uint3(src_args[src_at], src_args[src_at + 1u], src_args[src_at + 2u]);
        uint3 g = min(asked, uint3(65535u, 65535u, 65535u));
        // In parts: one dimension, and not the garbage of an argument
        // nothing wrote.
        if (split && g.y <= 1u && g.z <= 1u && asked.x <= SPLIT * 65535u) {
            g.x = asked.x;
        }
        // x * y fits 32 bits; the whole count may not.
        if (g.z != 0 && g.x * g.y > GROUPS / g.z) {
            g = uint3(0, 0, 0);
        }
        cost = g.x * g.y * g.z * 64u;
        groups = g;
    } else {
        // Mesh dispatches: the console's are one-dimensional.
        uint3 g = uint3(src_args[src_at], src_args[src_at + 1u], src_args[src_at + 2u]);
        // Ahead of its draw, a record over the most a dispatch runs is a
        // slot the title has not filled (the title leaves all
        // ones there): empty, and not counted below either.
        if (ahead && g.x > 65535u) {
            g = uint3(0, 0, 0);
        }
        // Diagnostics, in the budget buffer's next words: the largest
        // group count a mesh record asked for, and the groups asked in all.
        uint was;
        InterlockedMax(budget[1], g.x, was);
        InterlockedAdd(budget[2], min(g.x, 1u << 24), was);
        g = uint3(min(g.x, 65535u), min(g.y, 1u), min(g.z, 1u));
        cost = g.x * g.y * g.z * 128u;
        groups = g;
    }
    bool over = false;
    if (cost > 0) {
        uint before;
        InterlockedOr(budget[0], 0, before);
        while (before < BUDGET) {
            uint next = (BUDGET - before <= cost) ? BUDGET : (before + cost);
            uint prev;
            InterlockedCompareExchange(budget[0], before, next, prev);
            if (prev == before) break;
            before = prev;
        }
        over = before >= BUDGET || BUDGET - before < cost;
    }
    if (kind <= 1) {
        if (over) {
            draw_counts = uint2(0, 0);
        }
        dst_args[dst_at] = draw_counts.x;
        dst_args[dst_at + 1u] = draw_counts.y;
        for (uint off = 2u; off < stride_words; off += 1u) {
            dst_args[dst_at + off] = src_args[src_at + off];
        }
    } else {
        if (over) {
            groups = uint3(0, 0, 0);
        }
        if (split) {
            for (uint part = 0; part < SPLIT; part++) {
                uint before = part * 65535u;
                uint x = groups.x > before ? min(groups.x - before, 65535u) : 0u;
                dst_args[dst_at + part * 3u] = x;
                dst_args[dst_at + part * 3u + 1u] = groups.y;
                dst_args[dst_at + part * 3u + 2u] = groups.z;
            }
            return;
        }
        dst_args[dst_at] = groups.x;
        dst_args[dst_at + 1u] = groups.y;
        dst_args[dst_at + 2u] = groups.z;
        for (uint off = 3u; off < stride_words; off += 1u) {
            dst_args[dst_at + off] = src_args[src_at + off];
        }
    }
}
"#;

struct Clamp {
    root: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Clamp {}
// SAFETY: as above.
unsafe impl Sync for Clamp {}

fn clamp() -> Option<&'static Clamp> {
    static CLAMP: std::sync::OnceLock<Option<Clamp>> = std::sync::OnceLock::new();
    CLAMP
        .get_or_init(|| match make() {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "no argument clamp: indirect execution dropped");
                None
            }
        })
        .as_ref()
}

fn make() -> windows::core::Result<Clamp> {
    let device = &host().ok_or_else(windows::core::Error::empty)?.device;
    make_for(device)
}

fn make_for(device: &ID3D12Device) -> windows::core::Result<Clamp> {
    make_from(device, SOURCE)
}

/// A compute pipeline of `source` with the clamp's root signature (the
/// clamp itself, or the compare of [`window_check`]).
fn make_from(device: &ID3D12Device, source: &str) -> windows::core::Result<Clamp> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the source and names live through the call; out pointers ours.
    let compiled = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            s!("clamp"),
            None,
            None,
            s!("main"),
            s!("cs_5_1"),
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = compiled {
        if let Some(errors) = errors {
            // SAFETY: the blob's own bytes.
            let text = unsafe {
                std::slice::from_raw_parts(
                    errors.GetBufferPointer().cast::<u8>(),
                    errors.GetBufferSize(),
                )
            };
            tracing::warn!("argument clamp: {}", String::from_utf8_lossy(text));
        }
        return Err(e);
    }
    let code = code.ok_or_else(windows::core::Error::empty)?;
    let root = root_signature(device)?;
    // SAFETY: the blob's own bytes.
    unsafe {
        let bytes =
            std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize());
        let pso = device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: bytes.as_ptr().cast(),
                BytecodeLength: bytes.len(),
            },
            ..Default::default()
        })?;
        Ok(Clamp { root, pso })
    }
}

/// Constants b0 (six words), SRVs t0..t1, and UAVs u0..u1.
fn root_signature(device: &ID3D12Device) -> windows::core::Result<ID3D12RootSignature> {
    crate::host::root_signature_from_params(
        device,
        &[
            crate::host::root_param_constants(0, 6),
            crate::host::root_param_descriptor(D3D12_ROOT_PARAMETER_TYPE_SRV, 0),
            crate::host::root_param_descriptor(D3D12_ROOT_PARAMETER_TYPE_SRV, 1),
            crate::host::root_param_descriptor(D3D12_ROOT_PARAMETER_TYPE_UAV, 0),
            crate::host::root_param_descriptor(D3D12_ROOT_PARAMETER_TYPE_UAV, 1),
        ],
    )
}

/// Minimum record byte stride for a supported PC indirect argument `kind`.
fn min_stride(kind: u32) -> Option<u32> {
    match kind {
        0 => Some(16),
        1 => Some(20),
        2 | 10 => Some(12),
        _ => None,
    }
}

/// Validates record alignment and returns the scratch byte size `stride * records`.
fn validate_layout(
    (kind, stride, records): (u32, u32, u32),
    src_offset: u64,
    count_offset: Option<u64>,
) -> Option<u64> {
    let min = min_stride(kind)?;
    if stride < min || !stride.is_multiple_of(4) || !src_offset.is_multiple_of(4) {
        return None;
    }
    if count_offset.is_some_and(|off| !off.is_multiple_of(4)) {
        return None;
    }
    let bytes = u64::from(stride).checked_mul(u64::from(records))?;
    (bytes <= u64::from(u32::MAX)).then_some(bytes)
}

/// Records, in `list`, the non-destructive clamp of up to `records` records of
/// `kind` (PC argument numbering) `stride` bytes apart from `src` (and bounded
/// by the 32-bit count in `count` when given) into a device-local scratch
/// buffer, at offset `0`. The scratch buffer is left in `INDIRECT_ARGUMENT`
/// until the next clamp into it: executions read it with no barrier of
/// their own. Replaces the list's compute root signature and pipeline
/// (callers bind theirs after). `None` if the clamp or alignment is not
/// available.
pub(crate) fn bound(
    list: &HostList,
    src: (&ID3D12Resource, u64),
    count: Option<(&ID3D12Resource, u64)>,
    spec: (u32, u32, u32),
) -> Option<ID3D12Resource> {
    let bytes = validate_layout(spec, src.1, count.map(|(_, off)| off))?;
    let (c, budget, dst) = (clamp()?, list.budget()?, list.indirect_scratch(bytes)?);
    clamp_into(list, c, &dst, budget, src, count, spec, 0).then_some(dst)
}

/// [`bound`] for a window of records clamped ahead of the draws that will
/// read them (`crate::draw`'s windows; no count buffer). The records past
/// the ones the title filled are whatever the buffer holds, and each
/// record clamped charges the list's budget when it is clamped: a slot of
/// all ones, clamped to the most a mesh dispatch runs (65,535 groups),
/// charged a sixteenth of the budget, so sixteen of them emptied every
/// draw of the list after them (run 93: "the largest group count asked for
/// 4294967295", 43 to 153 such records a frame). Here a record that asks
/// for more than can run is left empty and charges nothing; a record
/// clamped alone is clamped and charged as before.
pub(crate) fn bound_ahead(
    list: &HostList,
    src: (&ID3D12Resource, u64),
    spec: (u32, u32, u32),
) -> Option<ID3D12Resource> {
    let bytes = validate_layout(spec, src.1, None)?;
    let (c, budget, dst) = (clamp()?, list.budget()?, list.indirect_scratch(bytes)?);
    clamp_into(list, c, &dst, budget, src, None, spec, AHEAD).then_some(dst)
}

/// The parts [`bound_split`] makes of a dispatch, the most groups of each,
/// and the bytes between them.
pub(crate) const SPLIT: u32 = 2;
pub(crate) const PART: u32 = 65_535;
pub(crate) const PART_BYTES: u64 = 12;

/// [`bound`] for one dispatch that a program with a group base runs
/// (`victory_shader::emit::USER_GROUP_BASE`): [`SPLIT`] records follow each
/// other in the scratch buffer, of which the first has the first [`PART`]
/// groups in x and the next the ones after (empty if there are none). The
/// console runs 115,200 groups in one dispatch (the title's final
/// gather of indirect light: 8 for each 16x16 tile of 2560x1440); clamped
/// to what the host runs, the tiles at the bottom of the screen kept what
/// they showed before.
pub(crate) fn bound_split(list: &HostList, src: (&ID3D12Resource, u64)) -> Option<ID3D12Resource> {
    let spec = (2, 12, 1);
    validate_layout(spec, src.1, None)?;
    let (c, budget) = (clamp()?, list.budget()?);
    let dst = list.indirect_scratch(u64::from(SPLIT) * PART_BYTES)?;
    clamp_into(list, c, &dst, budget, src, None, spec, IN_PARTS).then_some(dst)
}

/// Bits of the clamp's `has_count`.
const COUNTED: u32 = 1;
const AHEAD: u32 = 2;
const IN_PARTS: u32 = 4;
const TABLED: u32 = 8;

/// [`record_clamp_as`] into `list`'s scratch buffer `dst`, from the state
/// the list left it in; afterwards the list knows it is readable as
/// indirect arguments.
#[allow(clippy::too_many_arguments)]
fn clamp_into(
    list: &HostList,
    c: &Clamp,
    dst: &ID3D12Resource,
    budget: u64,
    src: (&ID3D12Resource, u64),
    count: Option<(&ID3D12Resource, u64)>,
    spec: (u32, u32, u32),
    how: u32,
) -> bool {
    let before = list.indirect_state(dst);
    let recorded = record_clamp_as(c, &list.list, (dst, before), budget, src, count, spec, how);
    if recorded {
        list.indirect_readable(dst);
    }
    recorded
}

/// Records the clamp into `dst`, which is in state `before` (common, or
/// indirect arguments after an earlier clamp into it), and leaves `dst` in
/// `INDIRECT_ARGUMENT`. `how`: 0, or [`AHEAD`], the records are a window
/// clamped ahead of their draws ([`bound_ahead`]), or [`IN_PARTS`], one
/// dispatch in parts ([`bound_split`]). False, with nothing recorded, if
/// the layout or the addresses do not fit the clamp.
#[allow(clippy::too_many_arguments)]
fn record_clamp_as(
    c: &Clamp,
    l: &ID3D12GraphicsCommandList,
    (dst, before): (&ID3D12Resource, D3D12_RESOURCE_STATES),
    budget: u64,
    (src, src_offset): (&ID3D12Resource, u64),
    count: Option<(&ID3D12Resource, u64)>,
    (kind, stride, records): (u32, u32, u32),
    how: u32,
) -> bool {
    let Some(bytes) = validate_layout(
        (kind, stride, records),
        src_offset,
        count.map(|(_, off)| off),
    ) else {
        return false;
    };
    // SAFETY: live D3D12 buffer resources.
    let (src_base, dst_gpu) = unsafe { (src.GetGPUVirtualAddress(), dst.GetGPUVirtualAddress()) };
    if !src_base.is_multiple_of(4) || !dst_gpu.is_multiple_of(4) || !budget.is_multiple_of(4) {
        return false;
    }
    let (src_gpu, args_offset) = match src_offset
        .checked_add(bytes)
        .filter(|&end| end <= u64::from(u32::MAX))
    {
        Some(_) => (src_base, src_offset as u32),
        None => {
            let Some(rebased) = src_base.checked_add(src_offset) else {
                return false;
            };
            (rebased, 0)
        }
    };
    let (count_gpu, has_count, count_offset) = match count {
        Some((buf, off)) => {
            // SAFETY: a live D3D12 buffer resource.
            let base = unsafe { buf.GetGPUVirtualAddress() };
            if !base.is_multiple_of(4) {
                return false;
            }
            match off.checked_add(4).filter(|&end| end <= u64::from(u32::MAX)) {
                Some(_) => (base, 1u32, off as u32),
                None => {
                    let Some(rebased) = base.checked_add(off) else {
                        return false;
                    };
                    (rebased, 1u32, 0u32)
                }
            }
        }
        None => (src_gpu, 0u32, 0u32),
    };
    let constants = [kind, stride, records, args_offset, has_count | how, count_offset];
    // SAFETY: an open list; our root signature and pipeline; live source,
    // count, scratch destination, and budget buffers. `src` and `count` stay
    // in `COMMON` / `GENERIC_READ` via implicit buffer promotion while `dst`
    // transitions `before -> UNORDERED_ACCESS -> INDIRECT_ARGUMENT`. Moving
    // it out of `INDIRECT_ARGUMENT` orders this clamp's writes after every
    // earlier execution's reads of the scratch buffer; nothing else writes it.
    // Diagnostics: counted; `VICTORY_BARRIER_TEST=indirect` leaves the two
    // waits for every earlier shader write out (the state changes stay).
    #[cfg(feature = "diagnostics")]
    crate::copystat::add(crate::copystat::C::IndirectBounded, 1);
    #[cfg(feature = "diagnostics")]
    let waits = crate::copystat::barrier_test() & 2 == 0;
    #[cfg(not(feature = "diagnostics"))]
    let waits = true;
    let first = [
        uav_barrier(),
        transition(dst, before, D3D12_RESOURCE_STATE_UNORDERED_ACCESS),
    ];
    let second = [
        uav_barrier(),
        transition(
            dst,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
        ),
    ];
    let skip = usize::from(!waits);
    unsafe {
        // Earlier shader writes (the title's argument and count passes) first.
        l.ResourceBarrier(&first[skip..]);
        if records > 0 {
            l.SetComputeRootSignature(&c.root);
            l.SetPipelineState(&c.pso);
            l.SetComputeRoot32BitConstants(0, 6, constants.as_ptr().cast(), 0);
            l.SetComputeRootShaderResourceView(1, src_gpu);
            l.SetComputeRootShaderResourceView(2, count_gpu);
            l.SetComputeRootUnorderedAccessView(3, dst_gpu);
            l.SetComputeRootUnorderedAccessView(4, budget);
            l.Dispatch(records.div_ceil(64), 1, 1);
        }
        l.ResourceBarrier(&second[skip..]);
    }
    true
}

/// The most records of a run ([`bound_run`]): the clamp's 64 threads a
/// group, so a run's pass is one group, and no more than 64 threads charge
/// the list's budget word at once. All the threads of a pass race for that
/// word, and where it lies decides what the race costs
/// (`VICTORY_INDIRECT_CARD_BUDGET`). Counted in saved frames of this title,
/// runs of at most 64 make 106 to 147 passes a frame and runs of at most
/// 256 make 86 to 126.
pub(crate) const RUN_MOST: u32 = 64;

/// A run of single records clamped by one pass ([`bound_run`]).
pub(crate) struct Run {
    /// The scratch buffer whose slots hold the run's records clamped.
    scratch: ID3D12Resource,
    /// The bytes of a slot: the argument words of the run's kind.
    slot: u32,
    /// The run's table in the list's upload memory: the word that counts
    /// its records, then where each lies in the argument buffer.
    table: usize,
    /// The slots taken so far.
    records: u32,
}

// SAFETY: a D3D12 object (free-threaded) and the list's own upload memory,
// written while the list is recorded, by the thread that records it.
unsafe impl Send for Run {}

impl Run {
    /// The scratch buffer the run's draws read their records from.
    pub(crate) fn scratch(&self) -> &ID3D12Resource {
        &self.scratch
    }

    /// The bytes of a slot.
    pub(crate) fn slot_bytes(&self) -> u32 {
        self.slot
    }

    /// For a draw that reads the record at `offset` of the run's argument
    /// buffer: where in the scratch buffer it finds the record clamped. The
    /// draw gets the run's next free slot, which the run's pass fills when
    /// the list runs. A slot for every draw, also of a record a draw before
    /// it read: the pass charges the list's budget for each slot, so a
    /// record drawn again is charged again, as a pass in front of each draw
    /// charged it. None when the run is full, or the record is not one a
    /// run takes ([`run_takes`]).
    pub(crate) fn place(&mut self, offset: u64) -> Option<u64> {
        let offset = u32::try_from(offset).ok().filter(|o| o.is_multiple_of(4))?;
        let at = self.records;
        if at >= RUN_MOST {
            return None;
        }
        // SAFETY: the table holds the count and `RUN_MOST` offsets, and
        // the list that reads it has not been submitted.
        unsafe {
            let words = self.table as *mut u32;
            words.add(1 + at as usize).write_volatile(offset);
            words.write_volatile(at + 1);
        }
        self.records = at + 1;
        Some(u64::from(at) * u64::from(self.slot))
    }
}

/// Whether a run takes a single record of `kind` and `stride` at `offset` of
/// an argument buffer ([`bound_run`]): what the pass asks of a record,
/// checked before a pass is recorded for it.
pub(crate) fn run_takes(kind: u32, stride: u32, offset: u64) -> bool {
    min_stride(kind).is_some_and(|slot| stride >= slot)
        && offset.is_multiple_of(4)
        && u32::try_from(offset).is_ok_and(|o| o.checked_add(stride).is_some())
}

/// Records, in `list`, one pass that clamps a run of single records of
/// `kind` from the argument buffer `src` into the list's scratch buffer,
/// and leaves it readable as indirect arguments, as [`bound`] does. Which
/// records: the ones the draws after it ask for ([`Run::place`]), up to
/// [`RUN_MOST`]. The pass finds them by a table in the list's upload
/// memory, which is complete when the list is submitted, so it clamps
/// records that were asked for after it was recorded. Each draw has a slot
/// of its own, and each slot's record is clamped and charged to the list's
/// budget as one clamped alone in front of its draw.
///
/// The caller ends the run before it records anything that may write
/// arguments, or that clamps into the list's scratch buffer: the pass runs
/// where it is recorded, so it reads every record as it is after the last
/// such command before the run, and the run's draws all come before the
/// next one. That is what a pass in front of each draw read.
///
/// Why: the title issues its mesh draws one record a call, in runs of up
/// to some hundreds between two compute passes, each record somewhere else
/// in one large argument buffer. A window of records at one step
/// (`crate::draw`'s windows) serves under half of them. Counted in saved
/// frames of this title's opening: 2,600 to 3,400 single draw records a
/// frame lie in 106 to 147 runs of at most 64.
///
/// `None` if the clamp is not available or the buffers do not fit it.
pub(crate) fn bound_run(list: &HostList, src: &ID3D12Resource, kind: u32) -> Option<Run> {
    let slot = min_stride(kind)?;
    let (c, budget) = (clamp()?, list.budget()?);
    let (table_gpu, table) = list.scratch(4 * (1 + u64::from(RUN_MOST)))?;
    // SAFETY: upload memory of the list, just taken: no record yet.
    unsafe { table.cast::<u32>().write_volatile(0) };
    let dst = list.indirect_scratch(u64::from(RUN_MOST) * u64::from(slot))?;
    let before = list.indirect_state(&dst);
    if !record_run_as(
        c,
        &list.list,
        (&dst, before),
        budget,
        src,
        table_gpu,
        (kind, slot),
    ) {
        return None;
    }
    list.indirect_readable(&dst);
    Some(Run {
        scratch: dst,
        slot,
        table: table as usize,
        records: 0,
    })
}

/// Records the pass of a run ([`bound_run`]) into `dst`, which is in state
/// `before`, and leaves `dst` in `INDIRECT_ARGUMENT`: the word at
/// `table_gpu` counts the records, the words behind it name where each lies
/// in `src`, and record `i` is stored clamped at `i * slot`. False, with
/// nothing recorded, if the addresses do not fit the clamp.
fn record_run_as(
    c: &Clamp,
    l: &ID3D12GraphicsCommandList,
    (dst, before): (&ID3D12Resource, D3D12_RESOURCE_STATES),
    budget: u64,
    src: &ID3D12Resource,
    table_gpu: u64,
    (kind, slot): (u32, u32),
) -> bool {
    // SAFETY: live D3D12 buffer resources.
    let (src_gpu, dst_gpu) = unsafe { (src.GetGPUVirtualAddress(), dst.GetGPUVirtualAddress()) };
    if min_stride(kind) != Some(slot)
        || !src_gpu.is_multiple_of(4)
        || !dst_gpu.is_multiple_of(4)
        || !budget.is_multiple_of(4)
        || !table_gpu.is_multiple_of(4)
    {
        return false;
    }
    let constants = [kind, slot, RUN_MOST, 0u32, COUNTED | TABLED, 0u32];
    // Diagnostics: counted with the other passes of ours; as for them,
    // `VICTORY_BARRIER_TEST=indirect` leaves the two waits out.
    #[cfg(feature = "diagnostics")]
    crate::copystat::add(crate::copystat::C::IndirectBounded, 1);
    #[cfg(feature = "diagnostics")]
    let waits = crate::copystat::barrier_test() & 2 == 0;
    #[cfg(not(feature = "diagnostics"))]
    let waits = true;
    let first = [
        uav_barrier(),
        transition(dst, before, D3D12_RESOURCE_STATE_UNORDERED_ACCESS),
    ];
    let second = [
        uav_barrier(),
        transition(
            dst,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
        ),
    ];
    let skip = usize::from(!waits);
    // SAFETY: an open list; our root signature and pipeline; live source,
    // table, scratch destination and budget buffers. The first wait puts
    // the pass behind every earlier shader write (the title's argument
    // passes); the second puts the run's draws behind the pass.
    unsafe {
        l.ResourceBarrier(&first[skip..]);
        l.SetComputeRootSignature(&c.root);
        l.SetPipelineState(&c.pso);
        l.SetComputeRoot32BitConstants(0, 6, constants.as_ptr().cast(), 0);
        l.SetComputeRootShaderResourceView(1, src_gpu);
        l.SetComputeRootShaderResourceView(2, table_gpu);
        l.SetComputeRootUnorderedAccessView(3, dst_gpu);
        l.SetComputeRootUnorderedAccessView(4, budget);
        l.Dispatch(RUN_MOST.div_ceil(64), 1, 1);
        l.ResourceBarrier(&second[skip..]);
    }
    true
}

/// Whether every draw served from a window or a run is checked against a
/// fresh clamp of its record (`VICTORY_INDIRECT_WINDOW_CHECK=1`, with
/// `VICTORY_INDIRECT_WINDOW=1` or `VICTORY_INDIRECT_RUNS=1`; off by default).
/// A window clamps its records once, ahead of their draws, and a run's pass
/// clamps them in front of the first; if anything wrote a record between
/// that clamp and its draw, the draw reads the old one. The check
/// clamps the record again just before the draw (into a buffer of its own,
/// charging a budget of its own) and compares the two on the card: the
/// counts are logged ("window check"), and the first record that differs in
/// each run of a list is named. For telling whether windows can be trusted
/// in a scene before they are on by default; it costs a clamp and a compare
/// per window draw.
pub(crate) fn window_check() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_INDIRECT_WINDOW_CHECK").is_some_and(|v| v == "1"))
}

/// The compare of [`window_check`]: `words` dwords of the record a window
/// clamped (`t0`) against the same record clamped just now (`t1`). Counts
/// the checks and the records that differ in `u0`, keeps the first record
/// that differs there (eight dwords of each), and zeroes the fresh clamp's
/// own budget (`u1`) for the next check. It takes the clamp's root
/// signature (its first constant is `words`), and like the clamp it is
/// written for both compilers `D3DCompile` may be.
const CHECK_SOURCE: &str = r#"
cbuffer Check : register(b0) {
    uint words;
};
StructuredBuffer<uint> window_record : register(t0);
StructuredBuffer<uint> fresh_record : register(t1);
RWStructuredBuffer<uint> results : register(u0);
RWStructuredBuffer<uint> fresh_budget : register(u1);
[numthreads(1, 1, 1)]
void main() {
    bool same = true;
    for (uint i = 0; i < words; i++) {
        if (window_record[i] != fresh_record[i]) {
            same = false;
        }
    }
    uint was;
    InterlockedAdd(results[1], 1u, was);
    if (!same) {
        uint before;
        InterlockedAdd(results[0], 1u, before);
        if (before == 0) {
            for (uint k = 0; k < min(words, 8u); k++) {
                results[2u + k] = window_record[k];
                results[10u + k] = fresh_record[k];
            }
        }
    }
    fresh_budget[0] = 0u;
    fresh_budget[1] = 0u;
    fresh_budget[2] = 0u;
}
"#;

fn compare() -> Option<&'static Clamp> {
    static COMPARE: std::sync::OnceLock<Option<Clamp>> = std::sync::OnceLock::new();
    COMPARE
        .get_or_init(|| {
            let device = &host()?.device;
            match make_from(device, CHECK_SOURCE) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(error = %e, "no window check");
                    None
                }
            }
        })
        .as_ref()
}

/// A list's buffers for [`window_check`]: the fresh clamps' records and
/// budget (the card's memory), and the results (memory the CPU maps:
/// records that differed, checks, then the first differing record as the
/// window had it and as the fresh clamp made it, eight dwords each).
pub(crate) struct WindowCheck {
    args: ID3D12Resource,
    budget: ID3D12Resource,
    results: ID3D12Resource,
    data: usize,
}

// SAFETY: D3D12 objects are free-threaded; the mapping is read only at
// submission, when the list's last run has finished.
unsafe impl Send for WindowCheck {}
// SAFETY: as above.
unsafe impl Sync for WindowCheck {}

impl WindowCheck {
    pub(crate) fn new() -> Option<WindowCheck> {
        let host = host()?;
        let (results, data) = host.shared_buffer(256)?;
        // SAFETY: the mapping holds 256 bytes; nothing runs yet.
        unsafe { std::ptr::write_bytes(data, 0, 256) };
        Some(WindowCheck {
            args: host.uav_buffer(64 << 10)?,
            budget: host.uav_buffer(256)?,
            results,
            data: data as usize,
        })
    }

    /// What the list's last run found, zeroed for the next: checks, records
    /// that differed, and the first that did (as the window had it, as the
    /// fresh clamp made it). Only between runs of the list.
    pub(crate) fn take(&self) -> (u32, u32, [u32; 8], [u32; 8]) {
        let words = self.data as *mut u32;
        // SAFETY: the list's own mapped words; its last run has finished.
        unsafe {
            let at = |k: usize| words.add(k).read_volatile();
            let window = std::array::from_fn(|k| at(2 + k));
            let fresh = std::array::from_fn(|k| at(10 + k));
            let found = (at(1), at(0), window, fresh);
            std::ptr::write_bytes(words, 0, 18);
            found
        }
    }
}

/// Checks and differing records seen by [`window_check`] so far.
static CHECKED: [std::sync::atomic::AtomicU64; 2] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// [`window_check`] for a draw served from a window: the record of `kind`
/// at `src`, `step` bytes long, which the window clamped into `scratch` at
/// `at`, is clamped again into the list's check buffer and compared. Runs
/// compute work of ours, so the caller binds its pipeline again after it.
/// `ahead`: the record was clamped as one of a window, ahead of its draw
/// ([`bound_ahead`]); else as a record clamped alone, which one of a run
/// is ([`bound_run`]). False if nothing was recorded.
pub(crate) fn check_hit(
    list: &HostList,
    src: (&ID3D12Resource, u64),
    (kind, step): (u32, u32),
    (scratch, at): (&ID3D12Resource, u64),
    ahead: bool,
) -> bool {
    const CHECK_USE: usize = usize::MAX - 0x78;
    let (Some(c), Some(cmp), Some(check)) = (clamp(), compare(), list.window_check()) else {
        return false;
    };
    if list.first_use(CHECK_USE) {
        // At this recording's submission: what the list's last run found.
        let words = WindowCheck {
            args: check.args.clone(),
            budget: check.budget.clone(),
            results: check.results.clone(),
            data: check.data,
        };
        list.defer_host(move || {
            let (checked, differed, window, fresh) = words.take();
            if checked == 0 {
                return;
            }
            use std::sync::atomic::Ordering::Relaxed;
            let total = CHECKED[0].fetch_add(u64::from(checked), Relaxed) + u64::from(checked);
            let bad = CHECKED[1].fetch_add(u64::from(differed), Relaxed) + u64::from(differed);
            if differed > 0 {
                tracing::warn!(
                    differed,
                    checked,
                    ?window,
                    ?fresh,
                    "window check: a draw served from a window read a record that \
                     differs from a fresh clamp of it"
                );
            }
            if total / 10_000 != (total - u64::from(checked)) / 10_000 {
                tracing::info!(checked = total, differed = bad, "window check");
            }
        });
    }
    // SAFETY: live buffers.
    let (budget, results, fresh, window) = unsafe {
        (
            check.budget.GetGPUVirtualAddress(),
            check.results.GetGPUVirtualAddress(),
            check.args.GetGPUVirtualAddress(),
            scratch.GetGPUVirtualAddress() + at,
        )
    };
    // The record again, by the rules it was clamped by, into the check's
    // buffer (common before and after every check).
    let common = D3D12_RESOURCE_STATE_COMMON;
    let spec = (kind, step, 1);
    let how = if ahead { AHEAD } else { 0 };
    if !record_clamp_as(
        c,
        &list.list,
        (&check.args, common),
        budget,
        src,
        None,
        spec,
        how,
    ) {
        return false;
    }
    let args = D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
    let read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    let words = step / 4;
    // SAFETY: an open list; the compare's own pipeline and the clamp's root
    // signature; the scratch buffer is readable as arguments (and stays
    // so), the check's buffer was left so by the clamp above.
    unsafe {
        list.list.ResourceBarrier(&[
            transition(scratch, args, D3D12_RESOURCE_STATES(args.0 | read.0)),
            transition(&check.args, args, read),
        ]);
        list.list.SetComputeRootSignature(&cmp.root);
        list.list.SetPipelineState(&cmp.pso);
        list.list
            .SetComputeRoot32BitConstants(0, 1, (&raw const words).cast(), 0);
        list.list.SetComputeRootShaderResourceView(1, window);
        list.list.SetComputeRootShaderResourceView(2, fresh);
        list.list.SetComputeRootUnorderedAccessView(3, results);
        list.list.SetComputeRootUnorderedAccessView(4, budget);
        list.list.Dispatch(1, 1, 1);
        list.list.ResourceBarrier(&[
            transition(scratch, D3D12_RESOURCE_STATES(args.0 | read.0), args),
            transition(&check.args, read, common),
        ]);
    }
    true
}

fn uav_barrier() -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                pResource: std::mem::ManuallyDrop::new(None),
            }),
        },
    }
}

/// The benchmark of the clamp and its barriers (ignored by default).
#[cfg(test)]
mod bench;

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
    use windows::core::Interface;

    const WORKER_CS: &str = r#"
RWStructuredBuffer<uint> hits : register(u0);
[numthreads(1, 1, 1)]
void main() {
    uint prev;
    InterlockedAdd(hits[0], 1u, prev);
}
"#;

    struct WarpHarness {
        device: ID3D12Device,
        queue: ID3D12CommandQueue,
        allocator: ID3D12CommandAllocator,
        list: ID3D12GraphicsCommandList,
        fence: ID3D12Fence,
        submitted: u64,
        event: HANDLE,
        clamp: Clamp,
        worker: Clamp,
    }

    impl WarpHarness {
        fn new() -> Option<Self> {
            // SAFETY: plain DXGI WARP query and D3D12 test harness setup.
            unsafe {
                let factory: IDXGIFactory4 =
                    CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
                let adapter: IDXGIAdapter = factory.EnumWarpAdapter().ok()?;
                let desc = adapter.cast::<IDXGIAdapter1>().ok()?.GetDesc1().ok()?;
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                    return None;
                }
                let mut device: Option<ID3D12Device> = None;
                D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device).ok()?;
                let device = device?;
                let queue: ID3D12CommandQueue = device
                    .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                        Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                        ..Default::default()
                    })
                    .ok()?;
                let allocator: ID3D12CommandAllocator = device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                    .ok()?;
                let list: ID3D12GraphicsCommandList = device
                    .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                    .ok()?;
                let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).ok()?;
                let event = CreateEventW(None, false, false, None).ok()?;
                let clamp = make_for(&device).expect("clamp shader must compile on WARP");
                let worker = compile_cs(&device, WORKER_CS).expect("worker shader must compile");
                Some(Self {
                    device,
                    queue,
                    allocator,
                    list,
                    fence,
                    submitted: 0,
                    event,
                    clamp,
                    worker,
                })
            }
        }

        /// CPU-mapped L0 write-back buffer matching `Shadow`'s heap type.
        fn shared_buffer(&self, size: u64) -> (ID3D12Resource, *mut u8, u64) {
            let heap = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_CUSTOM,
                CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,
                MemoryPoolPreference: D3D12_MEMORY_POOL_L0,
                ..Default::default()
            };
            let desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Width: size.max(4),
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                ..Default::default()
            };
            let mut buffer: Option<ID3D12Resource> = None;
            let mut data = std::ptr::null_mut();
            // SAFETY: plain buffer creation and persistent CPU mapping.
            unsafe {
                self.device
                    .CreateCommittedResource(
                        &heap,
                        D3D12_HEAP_FLAG_NONE,
                        &desc,
                        D3D12_RESOURCE_STATE_COMMON,
                        None,
                        &mut buffer,
                    )
                    .unwrap();
                let buffer = buffer.unwrap();
                buffer.Map(0, None, Some(&mut data)).unwrap();
                let gpu = buffer.GetGPUVirtualAddress();
                (buffer, data.cast(), gpu)
            }
        }

        /// Device-local `DEFAULT`-heap UAV scratch buffer.
        fn default_uav(&self, size: u64) -> ID3D12Resource {
            let heap = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_DEFAULT,
                ..Default::default()
            };
            let desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Width: size.max(64 << 10),
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                ..Default::default()
            };
            let mut buffer: Option<ID3D12Resource> = None;
            // SAFETY: plain device-local buffer creation.
            unsafe {
                self.device
                    .CreateCommittedResource(
                        &heap,
                        D3D12_HEAP_FLAG_NONE,
                        &desc,
                        D3D12_RESOURCE_STATE_COMMON,
                        None,
                        &mut buffer,
                    )
                    .unwrap();
            }
            buffer.unwrap()
        }

        fn signature(&self, kind: u32, stride: u32) -> ID3D12CommandSignature {
            let arg = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE(kind as i32),
                ..Default::default()
            };
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: stride,
                NumArgumentDescs: 1,
                pArgumentDescs: &arg,
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            // SAFETY: valid single-argument command signature description.
            unsafe {
                self.device
                    .CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig)
                    .unwrap();
            }
            sig.unwrap()
        }

        fn submit_and_wait(&mut self) {
            self.submitted += 1;
            // SAFETY: closing, executing, waiting on fence, and resetting our list.
            unsafe {
                self.list.Close().unwrap();
                let cmd: ID3D12CommandList = self.list.cast().unwrap();
                self.queue.ExecuteCommandLists(&[Some(cmd)]);
                self.queue.Signal(&self.fence, self.submitted).unwrap();
                self.fence
                    .SetEventOnCompletion(self.submitted, self.event)
                    .unwrap();
                WaitForSingleObject(self.event, INFINITE);
                self.allocator.Reset().unwrap();
                self.list.Reset(&self.allocator, None).unwrap();
            }
        }
    }

    fn compile_cs(device: &ID3D12Device, source: &str) -> windows::core::Result<Clamp> {
        let mut code: Option<ID3DBlob> = None;
        // SAFETY: source and entry names outlive `D3DCompile`.
        unsafe {
            D3DCompile(
                source.as_ptr().cast(),
                source.len(),
                s!("worker"),
                None,
                None,
                s!("main"),
                s!("cs_5_1"),
                0,
                0,
                &mut code,
                None,
            )?;
            let code = code.unwrap();
            let bytes = std::slice::from_raw_parts(
                code.GetBufferPointer().cast::<u8>(),
                code.GetBufferSize(),
            );
            let root = crate::host::root_signature_from_params(
                device,
                &[crate::host::root_param_descriptor(
                    D3D12_ROOT_PARAMETER_TYPE_UAV,
                    0,
                )],
            )?;
            let pso = device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&root),
                CS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: bytes.as_ptr().cast(),
                    BytecodeLength: bytes.len(),
                },
                ..Default::default()
            })?;
            Ok(Clamp { root, pso })
        }
    }

    /// The compare of copies in the card's memory (`crate::host`), on the
    /// software adapter. Told 1 it flags the pieces whose bytes are not
    /// what was hashed last and leaves the hashes: a piece a shader changed
    /// and a later one changed back then looks untouched, which is right
    /// only if the piece was fetched after both. Told 2 (submissions sent
    /// ahead with the relay, whose pieces are fetched flight by flight) it
    /// stores the hash it made: each compare flags what changed since the
    /// compare before it, also a change back.
    #[test]
    fn warp_card_compare_stores_the_hash_it_made_when_told() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        let compare = {
            let mut code: Option<ID3DBlob> = None;
            // SAFETY: static HLSL source and valid out pointer.
            unsafe {
                D3DCompile(
                    crate::host::CARD_COMPARE.as_ptr().cast(),
                    crate::host::CARD_COMPARE.len(),
                    s!("card compare"),
                    None,
                    None,
                    s!("main"),
                    s!("cs_5_1"),
                    0,
                    0,
                    &mut code,
                    None,
                )
                .expect("the card compare must compile on WARP");
                let code = code.unwrap();
                let bytes = std::slice::from_raw_parts(
                    code.GetBufferPointer().cast::<u8>(),
                    code.GetBufferSize(),
                );
                let root = crate::host::card_compare_root_signature(&warp.device)
                    .expect("card compare root signature must create");
                let pso = warp
                    .device
                    .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                        pRootSignature: std::mem::transmute_copy(&root),
                        CS: D3D12_SHADER_BYTECODE {
                            pShaderBytecode: bytes.as_ptr().cast(),
                            BytecodeLength: bytes.len(),
                        },
                        ..Default::default()
                    })
                    .expect("card compare PSO must create");
                Clamp { root, pso }
            }
        };
        const PIECE: usize = 64 << 10;
        const PIECES: u32 = 3;
        let size = PIECE * PIECES as usize;
        let (_now, now_ptr, now_gpu) = warp.shared_buffer(size as u64);
        let (_hashes, _, hashes_gpu) = warp.shared_buffer(16 * u64::from(PIECES));
        let (_differs, differs_ptr, differs_gpu) = warp.shared_buffer(64);
        let content: Vec<u8> = (0..size).map(|i| (i * 31 + i / 251) as u8).collect();
        // SAFETY: the mapping holds `size` bytes.
        unsafe { std::ptr::copy_nonoverlapping(content.as_ptr(), now_ptr, size) };
        // One pass over the three pieces; what it left in `differs` (7s
        // where it stored nothing).
        let run = |warp: &mut WarpHarness, mode: u32| -> [u32; 3] {
            // SAFETY: our open list, live buffers, the compare's own root
            // signature; the mappings hold what is read and written.
            unsafe {
                for piece in 0..PIECES as usize {
                    differs_ptr.cast::<u32>().add(piece).write_volatile(7);
                }
                warp.list.SetComputeRootSignature(&compare.root);
                warp.list.SetPipelineState(&compare.pso);
                let constants = [0u32, 0, size as u32, PIECES, 0, mode];
                warp.list.SetComputeRoot32BitConstants(0, 6, constants.as_ptr().cast(), 0);
                warp.list.SetComputeRootShaderResourceView(1, now_gpu);
                warp.list.SetComputeRootUnorderedAccessView(2, hashes_gpu);
                warp.list.SetComputeRootUnorderedAccessView(3, differs_gpu);
                warp.list.Dispatch(PIECES.div_ceil(64), 1, 1);
            }
            warp.submit_and_wait();
            // SAFETY: as above.
            [0, 1, 2].map(|piece| unsafe { differs_ptr.cast::<u32>().add(piece).read_volatile() })
        };
        let at = PIECE + 1234;
        // SAFETY (each): inside the mapping.
        let set = |value: u8| unsafe { now_ptr.add(at).write_volatile(value) };
        // 0: the hashes are stored, nothing is flagged.
        assert_eq!(run(&mut warp, 0), [7, 7, 7]);
        assert_eq!(run(&mut warp, 1), [0, 0, 0]);
        // A change in the second piece: flagged, and with 1 flagged again
        // (the hash stays the old one).
        set(content[at] ^ 0x40);
        assert_eq!(run(&mut warp, 1), [0, 1, 0]);
        assert_eq!(run(&mut warp, 1), [0, 1, 0]);
        // Changed back: with 1 the piece looks untouched.
        set(content[at]);
        assert_eq!(run(&mut warp, 1), [0, 0, 0]);
        // With 2 the change is flagged once, and the hash is the new one.
        set(content[at] ^ 0x40);
        assert_eq!(run(&mut warp, 2), [0, 1, 0]);
        assert_eq!(run(&mut warp, 2), [0, 0, 0]);
        // Changed back: flagged, where 1 saw nothing.
        set(content[at]);
        assert_eq!(run(&mut warp, 2), [0, 1, 0]);
        assert_eq!(run(&mut warp, 2), [0, 0, 0]);
    }

    /// The clamp compiles with the system's compiler and the host takes it.
    #[test]
    fn argument_clamp_builds() {
        if super::host().is_none() {
            return; // no host GPU here
        }
        assert!(super::clamp().is_some());
    }

    #[test]
    fn rejects_unaligned_or_undersized_layouts() {
        assert_eq!(validate_layout((0, 16, 4), 0, None), Some(64));
        assert_eq!(validate_layout((1, 20, 4), 64, Some(128)), Some(80));
        assert_eq!(validate_layout((2, 12, 0), 0, Some(0)), Some(0));
        assert_eq!(validate_layout((10, 16, 2), 8, None), Some(32));
        // Undersized strides for each kind.
        assert_eq!(validate_layout((0, 12, 1), 0, None), None);
        assert_eq!(validate_layout((1, 16, 1), 0, None), None);
        assert_eq!(validate_layout((2, 8, 1), 0, None), None);
        assert_eq!(validate_layout((10, 8, 1), 0, None), None);
        // Unknown kind.
        assert_eq!(validate_layout((3, 16, 1), 0, None), None);
        // Unaligned stride, source offset, or count offset.
        assert_eq!(validate_layout((0, 18, 1), 0, None), None);
        assert_eq!(validate_layout((0, 16, 1), 2, None), None);
        assert_eq!(validate_layout((0, 16, 1), 0, Some(6)), None);
        // Total byte size exceeding 32-bit ByteAddressBuffer indexing.
        assert_eq!(validate_layout((1, 20, u32::MAX), 0, None), None);
    }

    /// WARP D3D12 execution test proving:
    /// (a) `countBuffer` with a small count (`3`) and `4,093` garbage tail slots
    ///     (`max = 4096`) does NOT blow `BUDGET` or zero valid dispatches in
    ///     this or subsequent `ExecuteIndirect` calls in the same list,
    /// (b) `src_args` (and `count_buf`) are NOT mutated in-place, and
    /// (c) full `stride` bytes (`stride = 24` > `12`) are copied for active
    ///     records into `dst_args` at offset `0`.
    #[test]
    fn warp_count_buffer_ignores_garbage_tail_and_preserves_source() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        const KIND_DISPATCH: u32 = 2;
        const STRIDE: u32 = 24; // 6 dwords: 3 dispatch group counts + 3 extra payload dwords
        const MAX: u32 = 4096;
        const ARGS_OFFSET: usize = 64; // non-zero offset with sentinel prefix
        const COUNT_OFFSET_1: usize = 128;
        const COUNT_OFFSET_2: usize = 192;

        let src_bytes = ARGS_OFFSET + (STRIDE as usize) * (MAX as usize) + 64;
        let (src_buf, src_ptr, _) = warp.shared_buffer(src_bytes as u64);
        let (count_buf, count_ptr, _) = warp.shared_buffer(512);
        let (_budget_buf, budget_ptr, budget_gpu) = warp.shared_buffer(256);
        let (_hits_buf, hits_ptr, hits_gpu) = warp.shared_buffer(256);
        let (readback_buf, readback_ptr, _) = warp.shared_buffer((STRIDE * 8) as u64);
        let dst_buf = warp.default_uav(u64::from(STRIDE) * u64::from(MAX));
        let sig = warp.signature(KIND_DISPATCH, STRIDE);

        // Fill the entire source buffer with non-zero sentinel bytes, then
        // populate 3 valid records at `ARGS_OFFSET` and 4,093 huge garbage
        // records in `3..MAX` that would exhaust `BUDGET` ~2,000x over if read.
        let mut initial_src = vec![0xA5u8; src_bytes];
        let write_record = |buf: &mut [u8], idx: usize, words: [u32; 6]| {
            let base = ARGS_OFFSET + idx * (STRIDE as usize);
            for (w, val) in words.into_iter().enumerate() {
                buf[base + w * 4..base + (w + 1) * 4].copy_from_slice(&val.to_le_bytes());
            }
        };
        write_record(
            &mut initial_src,
            0,
            [3, 2, 1, 0x1111_2222, 0x3333_4444, 0x5555_6666],
        );
        write_record(
            &mut initial_src,
            1,
            [5, 1, 1, 0x7777_8888, 0x9999_AAAA, 0xBBBB_CCCC],
        );
        // Record 2 exceeds 65,535 in X -> clamped to (65535, 1, 1).
        write_record(
            &mut initial_src,
            2,
            [100_000, 1, 1, 0xDDDD_EEEE, 0xFFFF_0001, 0x0123_4567],
        );
        for slot in 3..MAX as usize {
            // Each garbage slot would clamp to 65535 * 16 * 1 = 1,048,560 groups
            // (`cost = 67,107,840 ~= BUDGET / 2`), exhausting `BUDGET` immediately!
            write_record(
                &mut initial_src,
                slot,
                [0xFFFF_FFFF, 16, 1, 0xDEAD_BEEF, 0xBAD0_C0DE, slot as u32],
            );
        }

        // Second indirect call's source records (at offset 0 of a second buffer,
        // or later in `count_buf`): put 2 active records + garbage tail in a
        // second range to run back-to-back in the SAME command list.
        let mut initial_count = vec![0x5Au8; 512];
        initial_count[COUNT_OFFSET_1..COUNT_OFFSET_1 + 4].copy_from_slice(&3u32.to_le_bytes());
        initial_count[COUNT_OFFSET_2..COUNT_OFFSET_2 + 4].copy_from_slice(&2u32.to_le_bytes());

        // SAFETY: buffers are mapped with exact capacities above.
        unsafe {
            std::ptr::copy_nonoverlapping(initial_src.as_ptr(), src_ptr, src_bytes);
            std::ptr::copy_nonoverlapping(initial_count.as_ptr(), count_ptr, 512);
            (budget_ptr as *mut u32).write_volatile(0);
            (hits_ptr as *mut u32).write_volatile(0);
            std::ptr::write_bytes(readback_ptr, 0, (STRIDE * 8) as usize);
        }

        // Record Call #1 (`eff_count = 3` out of `MAX = 4096`), copy the first
        // 5 records of `dst_buf` out for inspection, then record Call #2
        // (`eff_count = 2` out of `MAX = 4096`) reusing `dst_buf` in the same list!
        assert!(record_clamp_as(
            &warp.clamp,
            &warp.list,
            (&dst_buf, D3D12_RESOURCE_STATE_COMMON),
            budget_gpu,
            (&src_buf, ARGS_OFFSET as u64),
            Some((&count_buf, COUNT_OFFSET_1 as u64)),
            (KIND_DISPATCH, STRIDE, MAX),
            0,
        ));
        // SAFETY: open command list, valid worker PSO/root and buffers.
        unsafe {
            warp.list.SetComputeRootSignature(&warp.worker.root);
            warp.list.SetPipelineState(&warp.worker.pso);
            warp.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
            warp.list.ExecuteIndirect(
                &sig,
                MAX,
                &dst_buf,
                0,
                Some(&count_buf),
                COUNT_OFFSET_1 as u64,
            );
        }

        // Snapshot `dst_buf` after Call #1 (copy 5 * STRIDE bytes, then leave
        // `dst_buf` readable as arguments again, as the clamp left it).
        // SAFETY: open command list; the clamp left `dst_buf` in `INDIRECT_ARGUMENT`.
        unsafe {
            warp.list.ResourceBarrier(&[transition(
                &dst_buf,
                D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            )]);
            warp.list
                .CopyBufferRegion(&readback_buf, 0, &dst_buf, 0, u64::from(STRIDE * 5));
            warp.list.ResourceBarrier(&[transition(
                &dst_buf,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
            )]);
        }

        // Call #2 in the SAME command list (`eff_count = 2` -> records 0 and 1:
        // 6 + 5 = 11 groups). Would run empty if Call #1's tail blew `BUDGET`!
        // The scratch buffer goes straight from arguments back to unordered
        // access.
        assert!(record_clamp_as(
            &warp.clamp,
            &warp.list,
            (&dst_buf, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT),
            budget_gpu,
            (&src_buf, ARGS_OFFSET as u64),
            Some((&count_buf, COUNT_OFFSET_2 as u64)),
            (KIND_DISPATCH, STRIDE, MAX),
            0,
        ));
        // SAFETY: open command list, valid worker PSO/root and buffers.
        unsafe {
            warp.list.SetComputeRootSignature(&warp.worker.root);
            warp.list.SetPipelineState(&warp.worker.pso);
            warp.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
            warp.list.ExecuteIndirect(
                &sig,
                MAX,
                &dst_buf,
                0,
                Some(&count_buf),
                COUNT_OFFSET_2 as u64,
            );
        }

        warp.submit_and_wait();

        // SAFETY: GPU execution finished; read mapped host results.
        let (hits, spent, after_src, after_count, clamped_words) = unsafe {
            let hits = (hits_ptr as *const u32).read_volatile();
            let spent = (budget_ptr as *const u32).read_volatile();
            let after_src = std::slice::from_raw_parts(src_ptr.cast_const(), src_bytes).to_vec();
            let after_count = std::slice::from_raw_parts(count_ptr.cast_const(), 512).to_vec();
            let words = std::slice::from_raw_parts(
                readback_ptr.cast::<u32>().cast_const(),
                (STRIDE as usize / 4) * 5,
            )
            .to_vec();
            (hits, spent, after_src, after_count, words)
        };

        // Call #1: (3*2*1) + (5*1*1) + (65535*1*1) = 65,546 groups.
        // Call #2: (3*2*1) + (5*1*1) = 11 groups.
        let expected_groups = (6 + 5 + 65_535) + (6 + 5);
        assert_eq!(
            hits, expected_groups,
            "valid indirect dispatches must all execute"
        );
        assert_eq!(
            spent,
            expected_groups * 64,
            "budget must reflect only active records below countBuffer"
        );
        assert_eq!(
            after_src, initial_src,
            "src_args must never be mutated in-place"
        );
        assert_eq!(
            after_count, initial_count,
            "count_buf must never be mutated"
        );

        // Check `dst_args` records 0..3: clamped group counts + preserved extra stride words.
        assert_eq!(
            &clamped_words[0..6],
            &[3, 2, 1, 0x1111_2222, 0x3333_4444, 0x5555_6666]
        );
        assert_eq!(
            &clamped_words[6..12],
            &[5, 1, 1, 0x7777_8888, 0x9999_AAAA, 0xBBBB_CCCC]
        );
        assert_eq!(
            &clamped_words[12..18],
            &[65_535, 1, 1, 0xDDDD_EEEE, 0xFFFF_0001, 0x0123_4567]
        );
        // Inactive slot 3 was never copied from `src_args`.
        assert_ne!(clamped_words[21], 0xDEAD_BEEF);
    }

    /// WARP D3D12 test proving `BUDGET` saturates without 32-bit unsigned
    /// wraparound across >64 huge indexed-draw records (`count = None`), while
    /// preserving `StartIndexLocation`, `BaseVertexLocation`,
    /// `StartInstanceLocation`, and custom stride padding, and leaving
    /// `src_args` untouched.
    /// One dispatch in parts: groups past what the host runs in one
    /// dimension go into the second record; what nothing wrote stays
    /// clamped, or empty.
    #[test]
    fn warp_large_dispatch_is_made_in_parts() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        let cases: [([u32; 3], [u32; 6]); 6] = [
            ([115_200, 1, 1], [65_535, 1, 1, 49_665, 1, 1]),
            ([65_535, 1, 1], [65_535, 1, 1, 0, 1, 1]),
            ([65_536, 1, 1], [65_535, 1, 1, 1, 1, 1]),
            ([5, 3, 2], [5, 3, 2, 0, 3, 2]),
            ([0, 1, 1], [0, 1, 1, 0, 1, 1]),
            // Past both parts: as before, what one dispatch runs.
            ([200_000, 1, 1], [65_535, 1, 1, 0, 1, 1]),
        ];
        for (asked, parts) in cases {
            let (src_buf, src_ptr, _) = warp.shared_buffer(16);
            let (_budget_buf, budget_ptr, budget_gpu) = warp.shared_buffer(256);
            let (readback_buf, readback_ptr, _) = warp.shared_buffer(64);
            let dst_buf = warp.default_uav(64);
            // SAFETY: mapped buffers of these sizes.
            unsafe {
                std::ptr::copy_nonoverlapping(asked.as_ptr().cast(), src_ptr, 12);
                std::ptr::write_bytes(budget_ptr, 0, 256);
                std::ptr::write_bytes(readback_ptr, 0, 64);
            }
            assert!(record_clamp_as(
                &warp.clamp,
                &warp.list,
                (&dst_buf, D3D12_RESOURCE_STATE_COMMON),
                budget_gpu,
                (&src_buf, 0),
                None,
                (2, 12, 1),
                IN_PARTS,
            ));
            // SAFETY: an open list and live buffers; the scratch buffer,
            // which the clamp left readable as arguments, is copied and
            // goes back to common for the next pass.
            unsafe {
                warp.list.ResourceBarrier(&[transition(
                    &dst_buf,
                    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
                warp.list
                    .CopyBufferRegion(&readback_buf, 0, &dst_buf, 0, 24);
                warp.list.ResourceBarrier(&[transition(
                    &dst_buf,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                    D3D12_RESOURCE_STATE_COMMON,
                )]);
            }
            warp.submit_and_wait();
            // SAFETY: the mapping holds 24 bytes the copy wrote.
            let made: [u32; 6] = unsafe { readback_ptr.cast::<[u32; 6]>().read_volatile() };
            assert_eq!(made, parts, "{asked:?}");
        }
    }

    /// A window of mesh records clamped ahead: the filled ones come out as
    /// they are and are charged, the slots of all ones come out empty and
    /// charge nothing; clamped as single records the same slots are clamped
    /// to the most and spend the budget.
    #[test]
    fn warp_window_leaves_unfilled_slots_empty_and_uncharged() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        const KIND_MESH: u32 = 10;
        const STRIDE: u32 = 32;
        const RECORDS: u32 = 16;
        let filled = [5u32, 0, 519];
        let total = (STRIDE * RECORDS) as usize;
        let mut src = vec![0xffu8; total];
        for (i, groups) in filled.into_iter().enumerate() {
            let words = [groups, 1, 1, 0xAB00 + i as u32];
            for (w, word) in words.into_iter().enumerate() {
                let at = i * STRIDE as usize + w * 4;
                src[at..at + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        for ahead in [true, false] {
            let (src_buf, src_ptr, _) = warp.shared_buffer(total as u64);
            let (_budget_buf, budget_ptr, budget_gpu) = warp.shared_buffer(256);
            let (readback_buf, readback_ptr, _) = warp.shared_buffer(total as u64);
            let dst_buf = warp.default_uav(total as u64);
            // SAFETY: mapped buffers of these sizes.
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), src_ptr, total);
                std::ptr::write_bytes(budget_ptr, 0, 256);
                std::ptr::write_bytes(readback_ptr, 0, total);
            }
            assert!(record_clamp_as(
                &warp.clamp,
                &warp.list,
                (&dst_buf, D3D12_RESOURCE_STATE_COMMON),
                budget_gpu,
                (&src_buf, 0),
                None,
                (KIND_MESH, STRIDE, RECORDS),
                if ahead { AHEAD } else { 0 },
            ));
            // SAFETY: an open list; the scratch buffer, which the clamp left
            // readable as arguments, is copied for the check and goes back
            // to common for the next pass.
            unsafe {
                warp.list.ResourceBarrier(&[transition(
                    &dst_buf,
                    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
                warp.list.CopyBufferRegion(&readback_buf, 0, &dst_buf, 0, total as u64);
                warp.list.ResourceBarrier(&[transition(
                    &dst_buf,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                    D3D12_RESOURCE_STATE_COMMON,
                )]);
            }
            warp.submit_and_wait();
            // SAFETY: the work is done; mapped buffers.
            let (spent, largest, out) = unsafe {
                (
                    (budget_ptr as *const u32).read_volatile(),
                    (budget_ptr as *const u32).add(1).read_volatile(),
                    std::slice::from_raw_parts(readback_ptr.cast::<u32>().cast_const(), total / 4).to_vec(),
                )
            };
            for (i, groups) in filled.into_iter().enumerate() {
                let rec = &out[i * 8..i * 8 + 4];
                assert_eq!(rec, [groups, u32::from(groups > 0).max(1).min(1), 1, 0xAB00 + i as u32], "filled record {i}, ahead {ahead}");
            }
            let charged_filled = (5 + 519) * 128;
            if ahead {
                for i in filled.len()..RECORDS as usize {
                    assert_eq!(out[i * 8..i * 8 + 3], [0, 0, 0], "unfilled slot {i} must come out empty");
                    assert_eq!(out[i * 8 + 3], u32::MAX, "the words after the record are copied as they are");
                }
                assert_eq!(spent, charged_filled, "unfilled slots charge nothing");
                assert_eq!(largest, 519, "and are not counted as asked for");
            } else {
                // Thirteen slots at 65,535 groups: sixteen would be all of it.
                assert_eq!(spent, charged_filled + 13 * 65535 * 128, "clamped one by one, each slot is charged the most");
                assert_eq!(largest, u32::MAX);
            }
        }
    }

    #[test]
    fn warp_budget_saturates_without_wraparound_and_preserves_draw_fields() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        const KIND_INDEXED: u32 = 1;
        const STRIDE: u32 = 24; // 5 indexed-draw words + 1 trailing custom word
        const RECORDS: u32 = 128; // 2 waves of 64: old InterlockedAdd wrapped 2^32 at 64!
        const BUDGET_CAP: u32 = 1 << 27;

        let total_bytes = (STRIDE * RECORDS) as usize;
        let (src_buf, src_ptr, _) = warp.shared_buffer(total_bytes as u64);
        let (_budget_buf, budget_ptr, budget_gpu) = warp.shared_buffer(256);
        let (readback_buf, readback_ptr, _) = warp.shared_buffer(total_bytes as u64);
        let dst_buf = warp.default_uav(total_bytes as u64);

        let mut initial_src = vec![0u8; total_bytes];
        for i in 0..RECORDS as usize {
            // The first record is no draw (2^24 indices): it runs empty
            // and charges nothing.
            let words: [u32; 6] = [
                if i == 0 { 1 << 24 } else { 1 << 22 }, // IndexCount
                if i == 0 { 1 } else { u32::MAX }, // InstanceCount -> clamps to 16 (cost = 1 << 26)
                1000 + i as u32,             // StartIndexLocation
                (-200i32 + i as i32) as u32, // BaseVertexLocation
                5000 + i as u32,             // StartInstanceLocation
                0xCAFE_0000 | (i as u32),    // Extra stride word
            ];
            let base = i * (STRIDE as usize);
            for (w, val) in words.into_iter().enumerate() {
                initial_src[base + w * 4..base + (w + 1) * 4].copy_from_slice(&val.to_le_bytes());
            }
        }

        // SAFETY: mapped buffers have sufficient capacity.
        unsafe {
            std::ptr::copy_nonoverlapping(initial_src.as_ptr(), src_ptr, total_bytes);
            (budget_ptr as *mut u32).write_volatile(0);
            std::ptr::write_bytes(readback_ptr, 0, total_bytes);
        }

        assert!(record_clamp_as(
            &warp.clamp,
            &warp.list,
            (&dst_buf, D3D12_RESOURCE_STATE_COMMON),
            budget_gpu,
            (&src_buf, 0),
            None,
            (KIND_INDEXED, STRIDE, RECORDS),
            0,
        ));

        // SAFETY: open command list; copy clamped scratch buffer (left
        // readable as arguments) for verification.
        unsafe {
            warp.list.ResourceBarrier(&[transition(
                &dst_buf,
                D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            )]);
            warp.list
                .CopyBufferRegion(&readback_buf, 0, &dst_buf, 0, total_bytes as u64);
            warp.list.ResourceBarrier(&[transition(
                &dst_buf,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_COMMON,
            )]);
        }
        warp.submit_and_wait();

        // SAFETY: GPU work completed.
        let (spent, after_src, out_words) = unsafe {
            let spent = (budget_ptr as *const u32).read_volatile();
            let after_src = std::slice::from_raw_parts(src_ptr.cast_const(), total_bytes).to_vec();
            let out_words = std::slice::from_raw_parts(
                readback_ptr.cast::<u32>().cast_const(),
                total_bytes / 4,
            )
            .to_vec();
            (spent, after_src, out_words)
        };

        assert_eq!(
            spent, BUDGET_CAP,
            "budget must saturate at BUDGET without wrapping"
        );
        assert_eq!(after_src, initial_src, "src_args must remain untouched");

        let mut kept = 0usize;
        let mut zeroed = 0usize;
        for i in 0..RECORDS as usize {
            let rec = &out_words[i * 6..(i + 1) * 6];
            if rec[0] == (1 << 22) && rec[1] == 16 {
                assert_ne!(i, 0, "a record of 2^24 indices is no draw");
                kept += 1;
            } else {
                assert_eq!((rec[0], rec[1]), (0, 0), "excess draw must be zeroed");
                zeroed += 1;
            }
            assert_eq!(rec[2], 1000 + i as u32);
            assert_eq!(rec[3], (-200i32 + i as i32) as u32);
            assert_eq!(rec[4], 5000 + i as u32);
            assert_eq!(rec[5], 0xCAFE_0000 | (i as u32));
        }
        assert_eq!((kept, zeroed), (2, (RECORDS as usize) - 2));
    }

    /// Exercises `HostList::indirect_scratch`, `bound`, `Host::execute`, and
    /// `HostList::reset` end-to-end on the host device.
    #[test]
    fn host_list_reuses_and_grows_scratch_across_submissions() {
        let Some(host) = super::host() else {
            return;
        };
        let Ok(list) = host.new_list() else {
            return;
        };
        let Some((src_buf, src_ptr)) = host.shared_buffer(256 << 10) else {
            return;
        };
        let Some((count_buf, count_ptr)) = host.shared_buffer(256) else {
            return;
        };
        // SAFETY: mapped host buffers.
        unsafe {
            std::ptr::write_bytes(src_ptr, 0x7F, 256 << 10);
            (count_ptr as *mut u32).write_volatile(2);
        }

        // First two calls fit inside the initial 64 KiB scratch buffer and
        // must reuse the exact same `ID3D12Resource`.
        let b1 = bound(&list, (&src_buf, 0), Some((&count_buf, 0)), (0, 16, 64)).unwrap();
        let b2 = bound(&list, (&src_buf, 64), Some((&count_buf, 0)), (1, 20, 128)).unwrap();
        assert_eq!(
            b1.as_raw(),
            b2.as_raw(),
            "scratch buffer within capacity must be reused"
        );

        // A call needing >64 KiB grows the scratch buffer mid-list while keeping
        // `b1` alive until reset.
        let b3 = bound(&list, (&src_buf, 0), Some((&count_buf, 0)), (1, 20, 5000)).unwrap();
        assert_ne!(
            b3.as_raw(),
            b1.as_raw(),
            "larger request must grow scratch buffer"
        );

        list.close();
        host.execute(&[&list]);
        list.reset();

        // After reset, requests up to the grown high-water mark reuse `b3` with
        // zero new allocations.
        let b4 = bound(&list, (&src_buf, 0), Some((&count_buf, 0)), (1, 20, 4096)).unwrap();
        assert_eq!(
            b4.as_raw(),
            b3.as_raw(),
            "grown scratch buffer must persist across reset"
        );
        list.close();
        host.execute(&[&list]);
        list.reset();
    }

    /// On the host card: a draw record that asks for 2^23 vertices an
    /// instance or more runs empty and charges nothing. Twenty of them
    /// spent the list's budget before (sixteen ran, 2^27 vertices in all)
    /// and emptied the real draw clamped after them.
    #[test]
    fn host_list_draw_of_too_many_vertices_runs_empty_and_charges_nothing() {
        let (Some(host), Some(c)) = (super::host(), clamp()) else {
            return;
        };
        let Ok(list) = host.new_list() else {
            return;
        };
        const KIND_DRAW: u32 = 0;
        const STEP: u32 = 32;
        const BIG: usize = 20;
        // Count and instances; the two words after them are the record's own.
        let big: Vec<[u32; 4]> = (0..BIG as u32)
            .map(|i| {
                [
                    if i % 2 == 0 { 1 << 23 } else { 1 << 24 },
                    1,
                    100 + i,
                    200 + i,
                ]
            })
            .collect();
        let real = [[(1u32 << 23) - 3, 1, 7, 8], [6, 4, 9, 10]];
        let fill = |records: &[[u32; 4]]| -> Option<ID3D12Resource> {
            let (buffer, ptr) = host.shared_buffer(u64::from(STEP) * records.len() as u64)?;
            // SAFETY: the mapping holds a step for every record.
            unsafe {
                std::ptr::write_bytes(ptr, 0xff, STEP as usize * records.len());
                for (i, words) in records.iter().enumerate() {
                    std::ptr::copy_nonoverlapping(
                        words.as_ptr().cast::<u8>(),
                        ptr.add(i * STEP as usize),
                        16,
                    );
                }
            }
            Some(buffer)
        };
        let (big_bytes, real_bytes) = (u64::from(STEP) * BIG as u64, u64::from(STEP) * 2);
        let (
            Some(big_src),
            Some(real_src),
            Some(big_dst),
            Some(real_dst),
            Some((back, back_ptr)),
            Some(budget),
        ) = (
            fill(&big),
            fill(&real),
            host.uav_buffer(big_bytes),
            host.uav_buffer(real_bytes),
            host.shared_buffer(big_bytes + real_bytes),
            list.budget(),
        )
        else {
            return;
        };
        let common = D3D12_RESOURCE_STATE_COMMON;
        assert!(record_clamp_as(
            c,
            &list.list,
            (&big_dst, common),
            budget,
            (&big_src, 0),
            None,
            (KIND_DRAW, STEP, BIG as u32),
            0
        ));
        assert!(record_clamp_as(
            c,
            &list.list,
            (&real_dst, common),
            budget,
            (&real_src, 0),
            None,
            (KIND_DRAW, STEP, 2),
            0
        ));
        // SAFETY: an open list; the clamps left both buffers readable as
        // arguments, and the test holds them until the list has run.
        unsafe {
            let args = D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
            let source = D3D12_RESOURCE_STATE_COPY_SOURCE;
            list.list.ResourceBarrier(&[
                transition(&big_dst, args, source),
                transition(&real_dst, args, source),
            ]);
            list.list.CopyBufferRegion(&back, 0, &big_dst, 0, big_bytes);
            list.list
                .CopyBufferRegion(&back, big_bytes, &real_dst, 0, real_bytes);
            list.list.ResourceBarrier(&[
                transition(&big_dst, source, common),
                transition(&real_dst, source, common),
            ]);
        }
        list.close();
        host.execute(&[&list]);
        list.reset();
        // SAFETY: the list has run; the mapping holds both copies.
        let out: Vec<u32> = unsafe {
            std::slice::from_raw_parts(
                back_ptr.cast::<u32>().cast_const(),
                ((big_bytes + real_bytes) / 4) as usize,
            )
            .to_vec()
        };
        let record = |i: usize| &out[i * (STEP as usize / 4)..][..4];
        for (i, asked) in big.iter().enumerate() {
            assert_eq!(
                record(i),
                [0, 0, asked[2], asked[3]],
                "record {i} of {} vertices is no draw",
                asked[0]
            );
        }
        assert_eq!(
            record(BIG),
            real[0],
            "a draw just under the bound runs as it is"
        );
        assert_eq!(
            record(BIG + 1),
            real[1],
            "and the budget was not spent on the records before"
        );
    }

    /// The window check: a record nothing wrote since its window clamped
    /// it compares equal; one written on the card after the window's clamp
    /// (an untracked writer) is found, and named as the window had it and
    /// as it is now.
    #[test]
    fn host_list_window_check_finds_a_record_written_under_its_window() {
        let Some(host) = super::host() else {
            return;
        };
        let Ok(list) = host.new_list() else {
            return;
        };
        let _ = host.log_messages();
        const KIND_DISPATCH: u32 = 2;
        const STEP: u32 = 32;
        let records = [[3u32, 1, 1], [5, 1, 1], [7, 1, 1], [9, 1, 1]];
        let total = records.len() * STEP as usize;
        let (Some((src, src_ptr)), Some((patch, patch_ptr))) =
            (host.shared_buffer(total as u64), host.shared_buffer(64))
        else {
            return;
        };
        // SAFETY: mapped buffers of these sizes; nothing runs yet.
        unsafe {
            std::ptr::write_bytes(src_ptr, 0, total);
            for (i, words) in records.iter().enumerate() {
                std::ptr::copy_nonoverlapping(
                    words.as_ptr().cast::<u8>(),
                    src_ptr.add(i * STEP as usize),
                    12,
                );
            }
            std::ptr::copy_nonoverlapping([11u32, 2, 1].as_ptr().cast::<u8>(), patch_ptr, 12);
        }
        let spec = (KIND_DISPATCH, STEP, records.len() as u32);
        let scratch = bound_ahead(&list, (&src, 0), spec).unwrap();
        // Record 0: nothing wrote it.
        assert!(check_hit(
            &list,
            (&src, 0),
            (KIND_DISPATCH, STEP),
            (&scratch, 0),
            true
        ));
        // Record 1 is written on the card after the window's clamp.
        let read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        // SAFETY: an open list; `src` was read by the clamp (promoted to a
        // shader read), `patch` is common; both live until the list has run.
        unsafe {
            list.list
                .ResourceBarrier(&[transition(&src, read, D3D12_RESOURCE_STATE_COPY_DEST)]);
            list.list
                .CopyBufferRegion(&src, u64::from(STEP), &patch, 0, 12);
            list.list
                .ResourceBarrier(&[transition(&src, D3D12_RESOURCE_STATE_COPY_DEST, read)]);
        }
        let at = u64::from(STEP);
        assert!(check_hit(
            &list,
            (&src, at),
            (KIND_DISPATCH, STEP),
            (&scratch, at),
            true
        ));
        list.close();
        assert_eq!(host.log_messages(), None, "the debug layer found an error");
        host.execute(&[&list]);
        let (checked, differed, window, fresh) = list.window_check().unwrap().take();
        list.reset();
        assert_eq!(
            (checked, differed),
            (2, 1),
            "two checks, the written record differs"
        );
        assert_eq!(window[..3], [5, 1, 1], "as the window clamped it");
        assert_eq!(fresh[..3], [11, 2, 1], "as it is now");
    }

    /// Records the dispatch records `groups` (three words each, `step`
    /// bytes apart, the rest of each record ones) into a fresh mapped
    /// buffer of `warp`'s device.
    fn dispatch_records(warp: &WarpHarness, groups: &[u32], step: u32) -> ID3D12Resource {
        let total = groups.len() * step as usize;
        let (buffer, ptr, _) = warp.shared_buffer(total as u64);
        let mut bytes = vec![0xffu8; total];
        for (i, &g) in groups.iter().enumerate() {
            for (w, word) in [g, 1, 1].into_iter().enumerate() {
                let at = i * step as usize + w * 4;
                bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        // SAFETY: the mapping holds `total` bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, total) };
        buffer
    }

    /// Two windows in one list, as `crate::draw` records them: each clamps
    /// its records ahead, then one execution per record reads the scratch
    /// buffer with no barrier between them; the second clamp moves the
    /// scratch buffer straight from arguments back to unordered access.
    /// Every record runs once, with the group count it asked for.
    #[test]
    fn warp_window_executions_read_their_records_with_no_barrier_between() {
        let Some(mut warp) = WarpHarness::new() else {
            return;
        };
        const KIND_DISPATCH: u32 = 2;
        // The title names 12 bytes and keeps its records 32 apart.
        const STRIDE: u32 = 12;
        const STEP: u32 = 32;
        let windows: [&[u32]; 2] = [&[1, 2, 3, 4], &[10, 11, 12, 13]];
        let sources: Vec<ID3D12Resource> = windows
            .iter()
            .map(|g| dispatch_records(&warp, g, STEP))
            .collect();
        let (_budget_buf, budget_ptr, budget_gpu) = warp.shared_buffer(256);
        let (_hits_buf, hits_ptr, hits_gpu) = warp.shared_buffer(256);
        let dst_buf = warp.default_uav(u64::from(STEP) * 4);
        let sig = warp.signature(KIND_DISPATCH, STRIDE);
        // SAFETY: mapped buffers of these sizes.
        unsafe {
            std::ptr::write_bytes(budget_ptr, 0, 256);
            std::ptr::write_bytes(hits_ptr, 0, 256);
        }
        let mut state = D3D12_RESOURCE_STATE_COMMON;
        for (groups, src) in windows.iter().zip(&sources) {
            assert!(record_clamp_as(
                &warp.clamp,
                &warp.list,
                (&dst_buf, state),
                budget_gpu,
                (src, 0),
                None,
                (KIND_DISPATCH, STEP, groups.len() as u32),
                AHEAD,
            ));
            state = D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
            // SAFETY: an open list; the worker's root signature and
            // pipeline; the scratch buffer is readable as arguments.
            unsafe {
                warp.list.SetComputeRootSignature(&warp.worker.root);
                warp.list.SetPipelineState(&warp.worker.pso);
                warp.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
                for i in 0..groups.len() as u64 {
                    warp.list
                        .ExecuteIndirect(&sig, 1, &dst_buf, i * u64::from(STEP), None, 0);
                }
            }
        }
        warp.submit_and_wait();
        // SAFETY: the work is done; mapped buffers.
        let (hits, spent) = unsafe {
            (
                (hits_ptr as *const u32).read_volatile(),
                (budget_ptr as *const u32).read_volatile(),
            )
        };
        let expected: u32 = windows.iter().flat_map(|g| g.iter()).sum();
        assert_eq!(hits, expected, "every record of both windows runs once");
        assert_eq!(spent, expected * 64, "and is charged what it asked for");
    }

    /// The windows of `warp_window_executions_read_their_records_with_no_barrier_between`
    /// through a host list (`bound_ahead`, the list's tracked scratch state,
    /// `Host::execute`), twice: on the first recording the scratch buffer
    /// starts common, on the second (after the reset) it starts common
    /// again. With the debug layer on (`VICTORY_D3D12_DEBUG=1` in a
    /// diagnostics build) no barrier names a state the buffer is not in.
    #[test]
    fn host_list_window_executions_need_no_barrier_of_their_own() {
        let Some(host) = super::host() else {
            return;
        };
        // What earlier tests left in the debug layer's queue, if it is on.
        let _ = host.log_messages();
        let Ok(list) = host.new_list() else {
            return;
        };
        let worker = compile_cs(&host.device, WORKER_CS).expect("worker shader must compile");
        const KIND_DISPATCH: u32 = 2;
        const STRIDE: u32 = 12;
        const STEP: u32 = 32;
        let signature = {
            let arg = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE(KIND_DISPATCH as i32),
                ..Default::default()
            };
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: STRIDE,
                NumArgumentDescs: 1,
                pArgumentDescs: &arg,
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            // SAFETY: a valid single-argument command signature description.
            unsafe {
                host.device
                    .CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig)
                    .unwrap();
            }
            sig.unwrap()
        };
        let windows: [&[u32]; 2] = [&[1, 2, 3, 4], &[10, 11, 12, 13]];
        let mut sources = Vec::new();
        for groups in windows {
            let total = groups.len() * STEP as usize;
            let Some((buffer, ptr)) = host.shared_buffer(total as u64) else {
                return;
            };
            let mut bytes = vec![0xffu8; total];
            for (i, &g) in groups.iter().enumerate() {
                for (w, word) in [g, 1, 1].into_iter().enumerate() {
                    let at = i * STEP as usize + w * 4;
                    bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
                }
            }
            // SAFETY: the mapping holds `total` bytes.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, total) };
            sources.push(buffer);
        }
        let Some((hits_buf, hits_ptr)) = host.shared_buffer(256) else {
            return;
        };
        let expected: u32 = windows.iter().flat_map(|g| g.iter()).sum();
        // SAFETY: a live buffer.
        let hits_gpu = unsafe { hits_buf.GetGPUVirtualAddress() };
        for _ in 0..2 {
            // SAFETY: the mapping holds 256 bytes; nothing runs.
            unsafe { std::ptr::write_bytes(hits_ptr, 0, 256) };
            let mut last = None;
            for (groups, src) in windows.iter().zip(&sources) {
                let spec = (KIND_DISPATCH, STEP, groups.len() as u32);
                let scratch = bound_ahead(&list, (src, 0), spec).unwrap();
                assert_eq!(
                    list.indirect_state(&scratch),
                    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT
                );
                // SAFETY: an open list; the worker's root signature and
                // pipeline; buffers the test holds until the list has run.
                unsafe {
                    list.list.SetComputeRootSignature(&worker.root);
                    list.list.SetPipelineState(&worker.pso);
                    list.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
                    for i in 0..groups.len() as u64 {
                        let at = i * u64::from(STEP);
                        list.list
                            .ExecuteIndirect(&signature, 1, &scratch, at, None, 0);
                    }
                }
                last = Some(scratch);
            }
            list.close();
            let scratch = last.unwrap();
            assert_eq!(
                list.indirect_state(&scratch),
                D3D12_RESOURCE_STATE_COMMON,
                "a closed list leaves it common"
            );
            // With the debug layer on: nothing it objects to was recorded.
            // Read before the submission, which moves the messages to the log.
            assert_eq!(host.log_messages(), None, "the debug layer found an error");
            host.execute(&[&list]);
            list.reset();
            // SAFETY: the list has run; a mapped buffer.
            let hits = unsafe { (hits_ptr as *const u32).read_volatile() };
            assert_eq!(hits, expected, "every record of both windows runs once");
        }
    }

    #[test]
    fn a_run_takes_single_records_on_four_bytes() {
        // a mesh record named 12 bytes wide, an indexed draw of 20, a draw kept 32 apart
        assert!(run_takes(10, 12, 0x6d71550));
        assert!(run_takes(1, 20, 0));
        assert!(run_takes(0, 32, 0x40));
        // not on four bytes, too narrow for its kind, an unknown kind, past 4 GiB
        assert!(!run_takes(10, 12, 0x6d71552));
        assert!(!run_takes(10, 8, 0x40));
        assert!(!run_takes(1, 16, 0x40));
        assert!(!run_takes(7, 12, 0x40));
        assert!(!run_takes(10, 12, 1 << 32));
        assert!(!run_takes(10, 12, u64::from(u32::MAX) - 3));
    }

    /// On the host card, through a host list: the single records of a run,
    /// scattered through an argument buffer and asked for after the run's
    /// pass was recorded, are found clamped in the run's slots when the
    /// list runs. A record asked for twice has two slots; a full run gives
    /// no more; a second run in the list clamps what a dispatch between
    /// the two wrote; and the check against a fresh clamp at each draw
    /// finds no record changed. Twice, as a list is recorded again after
    /// its reset.
    #[test]
    fn host_list_a_runs_records_are_clamped_by_its_pass() {
        let Some(host) = super::host() else {
            return;
        };
        let _ = host.log_messages();
        let Ok(list) = host.new_list() else {
            return;
        };
        let worker = compile_cs(&host.device, WORKER_CS).expect("worker shader must compile");
        const KIND_DISPATCH: u32 = 2;
        const SLOT: u32 = 12;
        let signature = {
            let arg = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE(KIND_DISPATCH as i32),
                ..Default::default()
            };
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: SLOT,
                NumArgumentDescs: 1,
                pArgumentDescs: &arg,
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            // SAFETY: a valid single-argument command signature description.
            unsafe {
                host.device
                    .CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig)
                    .unwrap();
            }
            sig.unwrap()
        };
        // Offsets out of order and far apart. The last record asks for more
        // groups in one dispatch than the clamp lets through (2^20): it
        // runs empty.
        let records: [(u32, [u32; 3]); 5] = [
            (0x3350, [3, 1, 1]),
            (0x0040, [5, 2, 1]),
            (0x7ff0, [7, 1, 1]),
            (0x0010, [9, 1, 3]),
            (0x2000, [65535, 65535, 1]),
        ];
        // What the worker's dispatch between the two runs leaves at 0x0040
        // is not known to the test: the second run reads another record,
        // which the title's side (here: the CPU, before the list is
        // submitted) holds from the start.
        let later: (u32, [u32; 3]) = (0x5000, [2, 2, 2]);
        let total = 0x8000usize;
        let (Some((src, src_ptr)), Some((hits_buf, hits_ptr))) =
            (host.shared_buffer(total as u64), host.shared_buffer(256))
        else {
            return;
        };
        // SAFETY: mapped buffers of these sizes; nothing runs yet.
        unsafe {
            std::ptr::write_bytes(src_ptr, 0xff, total);
            for (at, words) in records.into_iter().chain([later]) {
                std::ptr::copy_nonoverlapping(
                    words.as_ptr().cast::<u8>(),
                    src_ptr.add(at as usize),
                    12,
                );
            }
        }
        // SAFETY: a live buffer.
        let hits_gpu = unsafe { hits_buf.GetGPUVirtualAddress() };
        let expected: u32 = records[..4]
            .iter()
            .chain([&later])
            .map(|(_, g)| g[0] * g[1] * g[2])
            .sum();
        let bind_worker = || {
            // SAFETY: an open list; the worker's root signature and
            // pipeline; a buffer the test holds until the list has run.
            unsafe {
                list.list.SetComputeRootSignature(&worker.root);
                list.list.SetPipelineState(&worker.pso);
                list.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
            }
        };
        for round in 0..2 {
            // SAFETY: the mapping holds 256 bytes; nothing runs.
            unsafe { std::ptr::write_bytes(hits_ptr, 0, 256) };
            // The pass first: it knows none of its records yet.
            let mut run = bound_run(&list, &src, KIND_DISPATCH).expect("a run on the host");
            assert_eq!(run.slot_bytes(), SLOT);
            assert_eq!(
                list.indirect_state(run.scratch()),
                D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT
            );
            let places: Vec<u64> = records
                .iter()
                .map(|(at, _)| run.place(u64::from(*at)).unwrap())
                .collect();
            assert_eq!(places, [0, 12, 24, 36, 48], "slots in the order asked");
            assert_eq!(
                run.place(u64::from(records[1].0)),
                Some(60),
                "a slot for every draw, also of a record drawn before: it is charged again"
            );
            assert_eq!(
                run.place(0x42),
                None,
                "a record off four bytes has no place in a run"
            );
            bind_worker();
            for (place, (at, _)) in places.iter().zip(records) {
                // SAFETY: as above; the run's scratch buffer is readable as
                // arguments.
                unsafe {
                    list.list
                        .ExecuteIndirect(&signature, 1, run.scratch(), *place, None, 0)
                };
                // The record against a fresh clamp of it, as a draw checks.
                assert!(check_hit(
                    &list,
                    (&src, u64::from(at)),
                    (KIND_DISPATCH, SLOT),
                    (run.scratch(), *place),
                    false
                ));
                bind_worker();
            }
            // A full run gives no more places.
            let mut full = bound_run(&list, &src, KIND_DISPATCH).unwrap();
            for i in 0..u64::from(RUN_MOST) {
                assert!(full.place(0x6000 + 16 * i).is_some());
            }
            assert_eq!(full.place(0x7000), None, "a run holds {RUN_MOST} records");
            // A dispatch, and behind it a second run for the record after.
            bind_worker();
            // SAFETY: as above.
            unsafe { list.list.Dispatch(1, 1, 1) };
            let mut second = bound_run(&list, &src, KIND_DISPATCH).unwrap();
            let place = second.place(u64::from(later.0)).unwrap();
            assert_eq!(place, 0, "a new run begins at its first slot");
            bind_worker();
            // SAFETY: as above.
            unsafe {
                list.list
                    .ExecuteIndirect(&signature, 1, second.scratch(), place, None, 0)
            };
            list.close();
            assert_eq!(
                list.indirect_state(second.scratch()),
                D3D12_RESOURCE_STATE_COMMON,
                "a closed list leaves it common"
            );
            assert_eq!(host.log_messages(), None, "the debug layer found an error");
            host.execute(&[&list]);
            let (checked, differed, ..) = list.window_check().unwrap().take();
            list.reset();
            // SAFETY: the list has run; a mapped buffer.
            let hits = unsafe { (hits_ptr as *const u32).read_volatile() };
            assert_eq!(
                hits,
                expected + 1,
                "round {round}: every record of both runs ran as clamped, and the dispatch between them"
            );
            assert_eq!(
                (checked, differed),
                (5, 0),
                "round {round}: the slots hold what a fresh clamp makes"
            );
        }
    }

    /// On the host card: a list's budget, in memory the CPU maps and in the
    /// card's own, is spent by the records clamped in the list, and empty
    /// again at the list's next submission. A record that the draws of a
    /// run read again and again spends it as so many records do.
    #[test]
    fn host_list_budget_is_spent_in_a_list_and_empty_at_its_next_submission() {
        let Some(host) = super::host() else {
            return;
        };
        let worker = compile_cs(&host.device, WORKER_CS).expect("worker shader must compile");
        const KIND_DISPATCH: u32 = 2;
        const STRIDE: u32 = 12;
        // Forty dispatches of 65,535 groups: thirty-two of them spend the
        // budget (2^27 threads at 64 a group) but for 2,048 threads.
        const BIG: u32 = 40;
        let signature = {
            let arg = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE(KIND_DISPATCH as i32),
                ..Default::default()
            };
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: STRIDE,
                NumArgumentDescs: 1,
                pArgumentDescs: &arg,
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            // SAFETY: a valid single-argument command signature description.
            unsafe {
                host.device
                    .CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig)
                    .unwrap();
            }
            sig.unwrap()
        };
        let (Some((big, big_ptr)), Some((small, small_ptr)), Some((hits_buf, hits_ptr))) = (
            host.shared_buffer(u64::from(BIG * STRIDE)),
            host.shared_buffer(64),
            host.shared_buffer(256),
        ) else {
            return;
        };
        // SAFETY: mapped buffers of these sizes; nothing runs yet.
        unsafe {
            for i in 0..BIG as usize {
                std::ptr::copy_nonoverlapping(
                    [65535u32, 1, 1].as_ptr().cast::<u8>(),
                    big_ptr.add(i * STRIDE as usize),
                    12,
                );
            }
            std::ptr::copy_nonoverlapping([3u32, 1, 1].as_ptr().cast::<u8>(), small_ptr, 12);
        }
        // SAFETY: a live buffer.
        let hits_gpu = unsafe { hits_buf.GetGPUVirtualAddress() };
        // The forty as forty records clamped by one pass, and as one record
        // that forty draws of a run read: each draw is charged.
        for (card, by_run) in [(false, false), (true, false), (false, true), (true, true)] {
            let Ok(list) = host.new_list() else {
                return;
            };
            if card && !list.budget_on_the_card() {
                continue;
            }
            let run = |spend: bool| -> u32 {
                // SAFETY: the mapping holds 256 bytes; nothing runs.
                unsafe { std::ptr::write_bytes(hits_ptr, 0, 256) };
                if spend && by_run {
                    let mut drawn = bound_run(&list, &big, KIND_DISPATCH).unwrap();
                    for _ in 0..BIG {
                        drawn.place(0).unwrap();
                    }
                } else if spend {
                    bound(&list, (&big, 0), None, (KIND_DISPATCH, STRIDE, BIG)).unwrap();
                }
                let scratch = bound(&list, (&small, 0), None, (KIND_DISPATCH, STRIDE, 1)).unwrap();
                // SAFETY: an open list; the worker's root signature and
                // pipeline; buffers the test holds until the list has run.
                unsafe {
                    list.list.SetComputeRootSignature(&worker.root);
                    list.list.SetPipelineState(&worker.pso);
                    list.list.SetComputeRootUnorderedAccessView(0, hits_gpu);
                    list.list
                        .ExecuteIndirect(&signature, 1, &scratch, 0, None, 0);
                }
                list.close();
                host.execute(&[&list]);
                list.reset();
                // SAFETY: the list has run; a mapped buffer.
                unsafe { (hits_ptr as *const u32).read_volatile() }
            };
            for round in 0..2 {
                assert_eq!(
                    run(true),
                    0,
                    "card {card}, by a run {by_run}, round {round}: the spent budget empties the record after it"
                );
            }
            assert_eq!(
                run(false),
                3,
                "card {card}, by a run {by_run}: the budget is empty again at the next submission"
            );
        }
    }

    #[test]
    fn system_d3dcompile_builds_internal_hlsl() {
        for (name, src, entry, target) in [
            ("clamp", SOURCE, s!("main"), s!("cs_5_1")),
            ("card_compare", crate::host::CARD_COMPARE, s!("main"), s!("cs_5_1")),
            ("plane_corners", crate::host::PLANE_CORNERS, s!("corner"), s!("vs_5_1")),
            ("plane_steps", crate::host::PLANE_STEPS, s!("plane"), s!("ps_5_1")),
            ("plane_floats", crate::host::PLANE_FLOATS, s!("plane"), s!("ps_5_1")),
            ("worker_cs", WORKER_CS, s!("main"), s!("cs_5_1")),
            ("window_check", CHECK_SOURCE, s!("main"), s!("cs_5_1")),
        ] {
            let bytes = crate::host::compile(src, entry, target)
                .unwrap_or_else(|e| panic!("{name} failed D3DCompile: {e}"));
            assert!(!bytes.is_empty(), "{name} produced empty bytecode");
        }
    }
}
