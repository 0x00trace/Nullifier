//! The compare of copies in the card's memory (`crate::host::CARD_COMPARE`,
//! a thread a piece, and `CARD_COMPARE_LANES`, a group a piece) on the host
//! GPU: a test that each form finds every changed byte, and a benchmark of
//! the passes of one frame. The benchmark is ignored by default; run it with
//! `cargo test -p victory-gpu --lib indirect::card_hash -- --ignored --nocapture`.
//! It needs no title files.
//!
//! The older tests of the compare (`warp_card_compare_*` in this module's
//! tests) need the software adapter, which a Wine prefix does not give.

use super::*;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::core::Interface;

const PIECE: usize = 64 << 10;

/// A queue, a list and a fence of our own, and two timestamps.
struct Rig {
    queue: ID3D12CommandQueue,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    fence: ID3D12Fence,
    event: HANDLE,
    submitted: u64,
    stamps: ID3D12QueryHeap,
    back: ID3D12Resource,
    frequency: f64,
}

impl Rig {
    /// # Safety
    /// `device` is a live device.
    unsafe fn new(device: &ID3D12Device) -> Rig {
        // SAFETY: plain D3D12 object creation with valid descriptions.
        unsafe {
            let queue: ID3D12CommandQueue = device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                    ..Default::default()
                })
                .unwrap();
            let allocator: ID3D12CommandAllocator = device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                .unwrap();
            let list: ID3D12GraphicsCommandList = device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                .unwrap();
            list.Close().unwrap();
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            let event = CreateEventW(None, false, false, None).unwrap();
            let frequency = queue.GetTimestampFrequency().unwrap() as f64;
            let mut stamps: Option<ID3D12QueryHeap> = None;
            device
                .CreateQueryHeap(
                    &D3D12_QUERY_HEAP_DESC {
                        Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                        Count: 2,
                        NodeMask: 0,
                    },
                    &mut stamps,
                )
                .unwrap();
            let mut back: Option<ID3D12Resource> = None;
            device
                .CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_READBACK,
                        ..Default::default()
                    },
                    D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Width: 16,
                        Height: 1,
                        DepthOrArraySize: 1,
                        MipLevels: 1,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                        ..Default::default()
                    },
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut back,
                )
                .unwrap();
            Rig {
                queue,
                allocator,
                list,
                fence,
                event,
                submitted: 0,
                stamps: stamps.unwrap(),
                back: back.unwrap(),
                frequency,
            }
        }
    }

    /// Records a list with `record`, runs it and waits for it: the
    /// card's ms between a timestamp at its start and one at its end, and
    /// the ms from its submission to its end.
    ///
    /// # Safety
    /// What `record` records is valid, and what it names lives until
    /// this returns.
    unsafe fn run(&mut self, record: impl FnOnce(&ID3D12GraphicsCommandList)) -> [f64; 2] {
        // SAFETY: our own objects; the list's last run has ended (every run
        // is waited for here).
        unsafe {
            self.allocator.Reset().unwrap();
            self.list.Reset(&self.allocator, None).unwrap();
            self.list
                .EndQuery(&self.stamps, D3D12_QUERY_TYPE_TIMESTAMP, 0);
            record(&self.list);
            self.list
                .EndQuery(&self.stamps, D3D12_QUERY_TYPE_TIMESTAMP, 1);
            self.list.ResolveQueryData(
                &self.stamps,
                D3D12_QUERY_TYPE_TIMESTAMP,
                0,
                2,
                &self.back,
                0,
            );
            self.list.Close().unwrap();
            let running = std::time::Instant::now();
            self.queue
                .ExecuteCommandLists(&[Some(self.list.cast::<ID3D12CommandList>().unwrap())]);
            self.submitted += 1;
            self.queue.Signal(&self.fence, self.submitted).unwrap();
            self.fence
                .SetEventOnCompletion(self.submitted, self.event)
                .unwrap();
            WaitForSingleObject(self.event, INFINITE);
            let ran = running.elapsed().as_secs_f64() * 1e3;
            let mut data = std::ptr::null_mut();
            self.back.Map(0, None, Some(&mut data)).unwrap();
            let t = std::slice::from_raw_parts(data.cast::<u64>(), 2);
            let gpu = t[1].saturating_sub(t[0]) as f64 / self.frequency * 1e3;
            self.back.Unmap(0, None);
            [gpu, ran]
        }
    }
}

/// The compare made from `source`, as `Host::make_card_work` makes it.
fn compare_made(device: &ID3D12Device, source: &str) -> Clamp {
    let bytes = crate::host::compile(source, s!("main"), s!("cs_5_1"))
        .expect("the card compare must compile");
    let root = crate::host::card_compare_root_signature(device)
        .expect("the card compare's root signature");
    // SAFETY: the bytecode and the root signature live through the call.
    let pso = unsafe {
        device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: bytes.as_ptr().cast(),
                BytecodeLength: bytes.len(),
            },
            ..Default::default()
        })
    }
    .expect("the card compare's pipeline");
    Clamp { root, pso }
}

/// The two forms: whether a piece has a group, and the source.
const FORMS: [(bool, &str); 2] = [
    (false, crate::host::CARD_COMPARE),
    (true, crate::host::CARD_COMPARE_LANES),
];

/// One pass of `compare` over `pieces` pieces from `first`.
///
/// # Safety
/// An open list; the three addresses are those of live buffers large
/// enough for the span.
unsafe fn pass(
    list: &ID3D12GraphicsCommandList,
    lanes: bool,
    (now, hashes, differs): (u64, u64, u64),
    constants: [u32; 6],
) {
    // SAFETY: as the caller promises.
    unsafe {
        list.SetComputeRoot32BitConstants(0, 6, constants.as_ptr().cast(), 0);
        list.SetComputeRootShaderResourceView(1, now);
        list.SetComputeRootUnorderedAccessView(2, hashes);
        list.SetComputeRootUnorderedAccessView(3, differs);
        list.Dispatch(crate::host::card_compare_groups(constants[3], lanes), 1, 1);
    }
}

/// Each form of the compare finds every one-byte change, in the piece it is
/// in and in no other, with a copy that does not start on a piece boundary
/// and spans that do and do not start at piece 0. Told 1 it leaves the
/// hashes: a change stays flagged, and a change back is not flagged. Told 2
/// it stores what it made: a change is flagged once, and a change back is a
/// change.
#[test]
fn card_compare_finds_each_changed_byte_in_both_forms() {
    let Some(host) = super::host() else {
        eprintln!("no host GPU");
        return;
    };
    // Not a multiple of 16: the compare widens a piece to 16 bytes, by 10
    // in front and 6 behind here.
    const SKEW: u32 = 0x3456;
    const ALL: u32 = 6;
    let size = PIECE * ALL as usize - PIECE;
    for (lanes, source) in FORMS {
        // SAFETY: a live device; the lists below name buffers that live to
        // the end of this block, and every list is waited for.
        unsafe {
            let mut rig = Rig::new(&host.device);
            let compare = compare_made(&host.device, source);
            let (now, now_ptr) = host.shared_buffer(size as u64).unwrap();
            let (hashes, _) = host
                .shared_buffer(crate::shadow::card_hash_bytes_as(size as u64, lanes))
                .unwrap();
            let (differs, differs_ptr) = host.shared_buffer(64).unwrap();
            let content: Vec<u8> = (0..size).map(|i| (i * 131 + i / 7) as u8).collect();
            std::ptr::copy_nonoverlapping(content.as_ptr(), now_ptr, size);
            let addresses = (
                now.GetGPUVirtualAddress(),
                hashes.GetGPUVirtualAddress(),
                differs.GetGPUVirtualAddress(),
            );
            // A pass over the pieces from first; what it left in the
            // results, which hold 7 where it stored nothing. The form with a
            // group a piece only sets the dwords of pieces that differ, so
            // its compares start from zeroes, as a plan gives them.
            let mut run = |first: u32, pieces: u32, mode: u32| -> Vec<u32> {
                let before = if lanes && mode != 0 { 0 } else { 7 };
                for i in 0..pieces as usize {
                    differs_ptr.cast::<u32>().add(i).write_volatile(before);
                }
                rig.run(|list| {
                    list.SetComputeRootSignature(&compare.root);
                    list.SetPipelineState(&compare.pso);
                    pass(
                        list,
                        lanes,
                        addresses,
                        [first, SKEW, size as u32, pieces, 0, mode],
                    );
                });
                (0..pieces as usize)
                    .map(|i| differs_ptr.cast::<u32>().add(i).read_volatile())
                    .collect()
            };
            let only = |pieces: u32, i: u32| -> Vec<u32> {
                (0..pieces).map(|k| u32::from(k == i)).collect()
            };
            // Where piece p of the copy begins, as the compare cuts it.
            let begins = |p: u32| (p as usize * PIECE).saturating_sub(SKEW as usize);
            let flip = |at: usize, back: bool| {
                now_ptr
                    .add(at)
                    .write_volatile(content[at] ^ if back { 0 } else { 0x5a })
            };

            // The hashes are stored and nothing is flagged.
            assert_eq!(
                run(0, ALL, 0),
                vec![7; ALL as usize],
                "group a piece {lanes}: storing flags nothing"
            );
            assert_eq!(
                run(0, ALL, 1),
                vec![0; ALL as usize],
                "group a piece {lanes}: nothing changed"
            );
            // A span that starts at piece 1, and bytes at these places in
            // its pieces (16 bytes or more from a piece's ends, where the
            // widening lets a neighbour see them): the first and last steps,
            // both ends of the first KiB, the middle.
            const FIRST: u32 = 1;
            const PIECES: u32 = 4;
            for (k, step) in [16usize, 17, 31, 32, 1039, 1040, 4111, 30001, 65000, 65519]
                .into_iter()
                .enumerate()
            {
                let i = (k as u32) % PIECES;
                let at = begins(FIRST + i) + step;
                flip(at, false);
                assert_eq!(
                    run(FIRST, PIECES, 1),
                    only(PIECES, i),
                    "group a piece {lanes}: byte {at:#x} of piece {i}"
                );
                assert_eq!(
                    run(FIRST, PIECES, 1),
                    only(PIECES, i),
                    "group a piece {lanes}: told 1, the hash stays"
                );
                flip(at, true);
                assert_eq!(
                    run(FIRST, PIECES, 1),
                    vec![0; PIECES as usize],
                    "group a piece {lanes}: byte {at:#x} back"
                );
            }
            // The copy's piece 0 is short by the skew, and its last piece
            // ends at the buffer's end.
            for (piece, step) in [
                (0u32, 5usize),
                (0, PIECE - SKEW as usize - 20),
                (ALL - 1, 100),
                (ALL - 1, SKEW as usize - 20),
            ] {
                let at = begins(piece) + step;
                flip(at, false);
                assert_eq!(
                    run(0, ALL, 1),
                    only(ALL, piece),
                    "group a piece {lanes}: byte {at:#x} of piece {piece}"
                );
                flip(at, true);
                assert_eq!(run(0, ALL, 1), vec![0; ALL as usize]);
            }
            // Told 2: flagged once, then the new hash is the one stored.
            let at = begins(FIRST + 2) + 777;
            flip(at, false);
            assert_eq!(
                run(FIRST, PIECES, 2),
                only(PIECES, 2),
                "group a piece {lanes}: told 2, a change"
            );
            assert_eq!(
                run(FIRST, PIECES, 2),
                vec![0; PIECES as usize],
                "group a piece {lanes}: told 2, stored"
            );
            flip(at, true);
            assert_eq!(
                run(FIRST, PIECES, 2),
                only(PIECES, 2),
                "group a piece {lanes}: told 2, a change back"
            );
            assert_eq!(run(FIRST, PIECES, 1), vec![0; PIECES as usize]);
        }
    }
}

/// The passes of one frame in the title's opening on this tree, with each
/// form of the compare: hash passes after ranges were sent up (many, of a
/// few pieces each), compare passes (fewer, of many pieces), and a whole
/// copy hashed the first time. Medians of [`RUNS`] lists each.
#[test]
#[ignore = "benchmark; see the module notes"]
fn bench_card_hash_passes() {
    let Some(host) = super::host() else {
        eprintln!("no host GPU");
        return;
    };
    const RUNS: usize = 9;
    // A copy of 768 MB in the card's memory: every compare pass of the frame
    // has pieces of its own. (Passes over the same pieces, one after the
    // other, read part of them from the card's cache: 80 passes over a
    // third as many pieces took 0.30 ms with a group a piece.)
    const ALL: u32 = 12288;
    const HASH_PASSES: u32 = 520;
    const HASH_PIECES: u32 = 2;
    const COMPARE_PASSES: u32 = 80;
    const COMPARE_PIECES: u32 = 144;
    const _: () = assert!(COMPARE_PASSES * COMPARE_PIECES <= ALL);
    // The ranges sent up before the hash passes, one for each pass: 65 MB.
    const SENT_BYTES: u64 = (HASH_PIECES as u64) * PIECE as u64;
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum What {
        Nothing,
        SentUp,
        Hashes { barrier_every: u32 },
        Compares,
        Whole,
    }
    let size = u64::from(ALL) * PIECE as u64;
    // SAFETY: a live device; the lists name buffers that live to the end
    // of this function, and every list is waited for.
    unsafe {
        let mut rig = Rig::new(&host.device);
        let now = host.uav_buffer(size).unwrap();
        let (staging, staging_ptr) = host.shared_buffer(64 << 20).unwrap();
        for i in (0..64usize << 20).step_by(4096) {
            staging_ptr.add(i).write_volatile(i as u8);
        }
        let (differs, differs_ptr) = host.shared_buffer(1 << 20).unwrap();
        std::ptr::write_bytes(differs_ptr, 0, 1 << 20);
        let made: Vec<(bool, Clamp, ID3D12Resource)> = FORMS
            .iter()
            .map(|&(lanes, source)| {
                (
                    lanes,
                    compare_made(&host.device, source),
                    host.uav_buffer(crate::shadow::card_hash_bytes_as(size, lanes))
                        .unwrap(),
                )
            })
            .collect();
        let frame = |rig: &mut Rig, form: usize, what: What| -> [f64; 2] {
            let (lanes, compare, hashes) = &made[form];
            if what == What::Compares {
                // As a plan does for the form that only sets them.
                std::ptr::write_bytes(
                    differs_ptr,
                    0,
                    (COMPARE_PASSES * COMPARE_PIECES * 4) as usize,
                );
            }
            let addresses = (
                now.GetGPUVirtualAddress(),
                hashes.GetGPUVirtualAddress(),
                differs.GetGPUVirtualAddress(),
            );
            rig.run(|list| {
                if what == What::SentUp {
                    list.ResourceBarrier(&[transition(
                        &now,
                        D3D12_RESOURCE_STATE_COMMON,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                    )]);
                    for k in 0..HASH_PASSES {
                        let to = u64::from((k * 7919) % (ALL - HASH_PIECES)) * PIECE as u64;
                        let from = (u64::from(k) * SENT_BYTES) % ((64 << 20) - SENT_BYTES);
                        list.CopyBufferRegion(&now, to, &staging, from, SENT_BYTES);
                    }
                    list.ResourceBarrier(&[transition(
                        &now,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        D3D12_RESOURCE_STATE_COMMON,
                    )]);
                    return;
                }
                list.ResourceBarrier(&[transition(
                    &now,
                    D3D12_RESOURCE_STATE_COMMON,
                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                )]);
                list.SetComputeRootSignature(&compare.root);
                list.SetPipelineState(&compare.pso);
                match what {
                    What::Nothing | What::SentUp => {}
                    What::Hashes { barrier_every } => {
                        for k in 0..HASH_PASSES {
                            // As between two copies' passes in the layer's
                            // list: a transition there and back.
                            if barrier_every != 0 && k != 0 && k % barrier_every == 0 {
                                list.ResourceBarrier(&[transition(
                                    &now,
                                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                                    D3D12_RESOURCE_STATE_COPY_DEST,
                                )]);
                                list.ResourceBarrier(&[transition(
                                    &now,
                                    D3D12_RESOURCE_STATE_COPY_DEST,
                                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                                )]);
                            }
                            let first = (k * 7919) % (ALL - HASH_PIECES);
                            pass(
                                list,
                                *lanes,
                                addresses,
                                [first, 0, size as u32, HASH_PIECES, 0, 0],
                            );
                        }
                    }
                    What::Compares => {
                        for k in 0..COMPARE_PASSES {
                            let first = k * COMPARE_PIECES;
                            pass(
                                list,
                                *lanes,
                                addresses,
                                [
                                    first,
                                    0,
                                    size as u32,
                                    COMPARE_PIECES,
                                    k * COMPARE_PIECES * 4,
                                    1,
                                ],
                            );
                        }
                    }
                    What::Whole => pass(list, *lanes, addresses, [0, 0, size as u32, ALL, 0, 0]),
                }
                list.ResourceBarrier(&[transition(
                    &now,
                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                    D3D12_RESOURCE_STATE_COMMON,
                )]);
            })
        };
        let cases: [(&str, What); 6] = [
            ("an empty list", What::Nothing),
            ("65 MB sent up in 520 ranges", What::SentUp),
            (
                "520 hash passes of 2 pieces",
                What::Hashes { barrier_every: 0 },
            ),
            (
                "the same, 40 copies' passes",
                What::Hashes { barrier_every: 13 },
            ),
            ("80 compare passes of 144 pieces", What::Compares),
            ("768 MB hashed in one pass", What::Whole),
        ];
        // The hashes of every piece first (the compares then find nothing
        // changed, as nearly all of the title's do), then the cases in
        // turn, so drift falls on all alike.
        for form in 0..made.len() {
            frame(&mut rig, form, What::Whole);
            for &(_, what) in &cases {
                frame(&mut rig, form, what);
            }
        }
        let mut times: Vec<Vec<[f64; 2]>> = vec![Vec::new(); cases.len() * made.len()];
        for _ in 0..RUNS {
            for (i, &(_, what)) in cases.iter().enumerate() {
                for form in 0..made.len() {
                    let took = frame(&mut rig, form, what);
                    times[i * made.len() + form].push(took);
                }
            }
        }
        let flagged = (0..(COMPARE_PASSES * COMPARE_PIECES) as usize)
            .filter(|&i| differs_ptr.cast::<u32>().add(i).read_volatile() != 0)
            .count();
        let median = |v: &[[f64; 2]], k: usize| {
            let mut x: Vec<f64> = v.iter().map(|t| t[k]).collect();
            x.sort_by(f64::total_cmp);
            x[x.len() / 2]
        };
        eprintln!(
            "card hash passes on the host GPU, medians of {RUNS} lists: the card's ms by its timestamps, and ms from submission to done"
        );
        for (i, (name, _)) in cases.iter().enumerate() {
            let (a, b) = (&times[i * made.len()], &times[i * made.len() + 1]);
            eprintln!(
                "  {name:<32} a thread a piece {:7.2} ms ({:7.2}), a group a piece {:7.2} ms ({:7.2})",
                median(a, 0),
                median(a, 1),
                median(b, 0),
                median(b, 1),
            );
        }
        assert_eq!(
            flagged, 0,
            "nothing changed between the hashes and the compares"
        );
    }
}
