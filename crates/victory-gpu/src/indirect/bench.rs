//! A frame's worth of one-record indirect draws on the host GPU, clamped one
//! at a time and in windows, recorded as the layer did before (the scratch
//! buffer back to the common state after every execution) and as it does
//! now (readable until the next clamp). Ignored by default; run it with
//! `cargo test -p victory-gpu --lib indirect::bench -- --ignored --nocapture`.
//! It needs no title files: the draws are tiny triangles of ours.
//!
//! After fxceless/victory-fork f85101b (`indirect/bench.rs`), with the
//! cases this tree can record and more: no clamp at all, one clamp for the
//! whole frame, the budget in the card's own memory, and runs of records
//! found by a table (`bound_run`).

use super::*;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIFactory4,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::core::Interface;

/// Draws a frame (the title's frames hold about 3,000 to 3,500 indirect
/// executions on this tree), records a window holds, and how far apart the
/// records are (the title keeps them 32 bytes apart).
const DRAWS: u32 = 3200;
const WINDOW: u32 = 32;
const STEP: u32 = 32;
const RUNS: usize = 9;

/// A tiny triangle, with no root arguments.
const SHADERS: &str = r#"
float4 vs(uint v : SV_VertexID) : SV_Position {
    float2 p = float2((v & 1) ? 0.05 : 0.0, (v & 2) ? 0.05 : 0.0) - 0.5;
    return float4(p, 0.5, 1);
}
float4 ps() : SV_Target { return float4(1, 0.5, 0.25, 1); }
"#;

/// What is recorded around the draws of a frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clamps {
    /// No clamp: the draws read the title's records as they are. Not
    /// something the layer may do; the floor to measure against.
    None,
    /// A clamp for every `n` records, and after every execution the
    /// scratch buffer back to the common state (as before).
    Before(u32),
    /// A clamp for every `n` records; the scratch buffer stays readable
    /// as arguments until the next clamp, and is common at the end.
    Now(u32),
    /// A pass for every run of [`RUN_MOST`] records, which finds them by a
    /// table (`bound_run`).
    Runs,
}

#[test]
#[ignore = "benchmark; see the module notes"]
fn bench_indirect_draw_barriers() {
    let Some(host) = super::host() else {
        eprintln!("no host GPU");
        return;
    };
    let device = &host.device;
    let clamp = make_for(device).expect("the clamp");
    let (Ok(vs), Ok(ps)) = (
        crate::host::compile(SHADERS, s!("vs"), s!("vs_5_1")),
        crate::host::compile(SHADERS, s!("ps"), s!("ps_5_1")),
    ) else {
        eprintln!("the draw's shaders did not compile");
        return;
    };
    // SAFETY: plain D3D12 object creation with valid descriptions; the
    // objects outlive every list recorded and run below.
    unsafe {
        let adapter = {
            let luid = device.GetAdapterLuid();
            CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0))
                .and_then(|f| {
                    f.EnumAdapterByLuid::<windows::Win32::Graphics::Dxgi::IDXGIAdapter1>(luid)
                })
                .and_then(|a| a.GetDesc1())
                .map(|d| {
                    String::from_utf16_lossy(&d.Description)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .unwrap_or_else(|_| "an unnamed adapter".to_owned())
        };
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

        // The pipeline: tiny triangles into a small target.
        let root = crate::host::root_signature_from_params(device, &[]).unwrap();
        let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vs.as_ptr().cast(),
                BytecodeLength: vs.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: ps.as_ptr().cast(),
                BytecodeLength: ps.len(),
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        desc.BlendState.RenderTarget[0].RenderTargetWriteMask =
            D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8;
        desc.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
        let pso: ID3D12PipelineState = device.CreateGraphicsPipelineState(&desc).unwrap();
        let target_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: 256,
            Height: 256,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            ..Default::default()
        };
        let mut target: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &target_desc,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                None,
                &mut target,
            )
            .unwrap();
        let target = target.unwrap();
        let rtvs: ID3D12DescriptorHeap = device
            .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: 1,
                ..Default::default()
            })
            .unwrap();
        let rtv = rtvs.GetCPUDescriptorHandleForHeapStart();
        device.CreateRenderTargetView(&target, None, rtv);

        // The title's records: three vertices, one instance, 32 bytes apart.
        let (src, src_ptr) = host.shared_buffer(u64::from(DRAWS * STEP)).unwrap();
        for i in 0..DRAWS as usize {
            let words = [3u32, 1, 0, 0];
            std::ptr::copy_nonoverlapping(
                words.as_ptr().cast::<u8>(),
                src_ptr.add(i * STEP as usize),
                16,
            );
        }
        // The budget where a list keeps it (memory the CPU maps), and the
        // same words in the card's own memory (zeroed when made; the runs
        // below charge far less than the budget).
        let (budget, budget_ptr) = host.shared_buffer(256).unwrap();
        let budget_gpu = budget.GetGPUVirtualAddress();
        let card_budget = host.uav_buffer(256).unwrap();
        let card_budget_gpu = card_budget.GetGPUVirtualAddress();
        let scratch = host
            .uav_buffer(u64::from(DRAWS * STEP).max(64 << 10))
            .unwrap();
        // The tables of the runs: for each, the count of its records and
        // where each lies in the source.
        const TABLE: u32 = 4 * (1 + RUN_MOST);
        let run_count = DRAWS.div_ceil(RUN_MOST);
        let (tables, tables_ptr) = host.shared_buffer(u64::from(run_count * TABLE)).unwrap();
        for run in 0..run_count {
            let words = tables_ptr.add((run * TABLE) as usize).cast::<u32>();
            let records = RUN_MOST.min(DRAWS - run * RUN_MOST);
            words.write_volatile(records);
            for i in 0..records {
                words
                    .add(1 + i as usize)
                    .write_volatile((run * RUN_MOST + i) * STEP);
            }
        }
        let tables_gpu = tables.GetGPUVirtualAddress();
        let signature = {
            let arg = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
                ..Default::default()
            };
            let desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: 16,
                NumArgumentDescs: 1,
                pArgumentDescs: &arg,
                NodeMask: 0,
            };
            let mut sig: Option<ID3D12CommandSignature> = None;
            device
                .CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig)
                .unwrap();
            sig.unwrap()
        };
        let stamps: ID3D12QueryHeap = {
            let mut heap: Option<ID3D12QueryHeap> = None;
            device
                .CreateQueryHeap(
                    &D3D12_QUERY_HEAP_DESC {
                        Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                        Count: 2,
                        NodeMask: 0,
                    },
                    &mut heap,
                )
                .unwrap();
            heap.unwrap()
        };
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
        let back = back.unwrap();
        let mut submitted = 0u64;

        // One frame's list: GPU ms (timestamps around the list), CPU ms
        // recording it, ms from submission to the end of its run.
        let mut frame = |clamps: Clamps, card: bool| -> [f64; 3] {
            allocator.Reset().unwrap();
            list.Reset(&allocator, None).unwrap();
            let recording = std::time::Instant::now();
            list.EndQuery(&stamps, D3D12_QUERY_TYPE_TIMESTAMP, 0);
            list.OMSetRenderTargets(1, Some(&rtv), false, None);
            list.RSSetViewports(&[D3D12_VIEWPORT {
                Width: 256.0,
                Height: 256.0,
                MaxDepth: 1.0,
                ..Default::default()
            }]);
            list.RSSetScissorRects(&[RECT {
                left: 0,
                top: 0,
                right: 256,
                bottom: 256,
            }]);
            list.IASetPrimitiveTopology(
                windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            );
            list.SetGraphicsRootSignature(&root);
            let mut state = D3D12_RESOURCE_STATE_COMMON;
            match clamps {
                Clamps::None => {
                    list.SetPipelineState(&pso);
                    for k in 0..DRAWS {
                        list.ExecuteIndirect(&signature, 1, &src, u64::from(k * STEP), None, 0);
                    }
                }
                Clamps::Runs => {
                    let slot = min_stride(0).unwrap();
                    for run in 0..run_count {
                        assert!(record_run_as(
                            &clamp,
                            &list,
                            (&scratch, state),
                            if card { card_budget_gpu } else { budget_gpu },
                            &src,
                            tables_gpu + u64::from(run * TABLE),
                            (0, slot),
                        ));
                        state = D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
                        list.SetPipelineState(&pso);
                        for k in 0..RUN_MOST.min(DRAWS - run * RUN_MOST) {
                            list.ExecuteIndirect(
                                &signature,
                                1,
                                &scratch,
                                u64::from(k * slot),
                                None,
                                0,
                            );
                        }
                    }
                }
                Clamps::Before(window) | Clamps::Now(window) => {
                    for first in (0..DRAWS).step_by(window as usize) {
                        let records = window.min(DRAWS - first);
                        assert!(record_clamp_as(
                            &clamp,
                            &list,
                            (&scratch, state),
                            if card { card_budget_gpu } else { budget_gpu },
                            (&src, u64::from(first * STEP)),
                            None,
                            (0, STEP, records),
                            if window > 1 { AHEAD } else { 0 },
                        ));
                        state = D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
                        list.SetPipelineState(&pso);
                        for k in 0..records {
                            list.ExecuteIndirect(
                                &signature,
                                1,
                                &scratch,
                                u64::from(k * STEP),
                                None,
                                0,
                            );
                            if matches!(clamps, Clamps::Before(_)) {
                                list.ResourceBarrier(&[transition(
                                    &scratch,
                                    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                                    D3D12_RESOURCE_STATE_COMMON,
                                )]);
                                state = D3D12_RESOURCE_STATE_COMMON;
                            }
                        }
                    }
                }
            }
            if state != D3D12_RESOURCE_STATE_COMMON {
                list.ResourceBarrier(&[transition(&scratch, state, D3D12_RESOURCE_STATE_COMMON)]);
            }
            list.EndQuery(&stamps, D3D12_QUERY_TYPE_TIMESTAMP, 1);
            list.ResolveQueryData(&stamps, D3D12_QUERY_TYPE_TIMESTAMP, 0, 2, &back, 0);
            list.Close().unwrap();
            let recorded = recording.elapsed().as_secs_f64() * 1e3;
            let running = std::time::Instant::now();
            queue.ExecuteCommandLists(&[Some(list.cast::<ID3D12CommandList>().unwrap())]);
            submitted += 1;
            queue.Signal(&fence, submitted).unwrap();
            fence.SetEventOnCompletion(submitted, event).unwrap();
            WaitForSingleObject(event, INFINITE);
            let ran = running.elapsed().as_secs_f64() * 1e3;
            let mut data = std::ptr::null_mut();
            back.Map(0, None, Some(&mut data)).unwrap();
            let t = std::slice::from_raw_parts(data.cast::<u64>(), 2);
            let gpu = t[1].saturating_sub(t[0]) as f64 / frequency * 1e3;
            back.Unmap(0, None);
            (budget_ptr as *mut u32).write_volatile(0);
            [gpu, recorded, ran]
        };

        let cases: [(&str, Clamps, bool); 11] = [
            ("no clamp", Clamps::None, false),
            ("one at a time, before", Clamps::Before(1), false),
            ("one at a time, now", Clamps::Now(1), false),
            ("windows of 32, before", Clamps::Before(WINDOW), false),
            ("windows of 32, now", Clamps::Now(WINDOW), false),
            ("one clamp for all", Clamps::Now(DRAWS), false),
            // The budget in the card's memory instead of memory the CPU
            // maps (where a list's budget is).
            ("one at a time, card", Clamps::Now(1), true),
            ("windows of 32, card", Clamps::Now(WINDOW), true),
            ("one clamp for all, card", Clamps::Now(DRAWS), true),
            ("runs of 64", Clamps::Runs, false),
            ("runs of 64, card", Clamps::Runs, true),
        ];
        // Warm up, then the cases in turn, so drift falls on all alike.
        for &(_, clamps, card) in &cases {
            frame(clamps, card);
        }
        let mut times: Vec<Vec<[f64; 3]>> = vec![Vec::new(); cases.len()];
        for _ in 0..RUNS {
            for (i, &(_, clamps, card)) in cases.iter().enumerate() {
                times[i].push(frame(clamps, card));
            }
        }
        let median = |v: &[[f64; 3]], k: usize| {
            let mut x: Vec<f64> = v.iter().map(|t| t[k]).collect();
            x.sort_by(f64::total_cmp);
            x[x.len() / 2]
        };
        eprintln!(
            "indirect draw barriers on {adapter}: {DRAWS} one-record draws a frame, medians of {RUNS} frames"
        );
        for (i, (name, ..)) in cases.iter().enumerate() {
            let gpu = median(&times[i], 0);
            eprintln!(
                "  {name:<22} GPU {gpu:7.2} ms ({:5.2} us a draw), recording {:6.2} ms, submit to done {:7.2} ms",
                gpu * 1e3 / f64::from(DRAWS),
                median(&times[i], 1),
                median(&times[i], 2),
            );
        }
        for (from, to) in [
            (1, 2),
            (3, 4),
            (1, 4),
            (2, 6),
            (4, 7),
            (2, 9),
            (4, 9),
            (9, 10),
        ] {
            let (a, b) = (median(&times[from], 0), median(&times[to], 0));
            eprintln!(
                "  {} -> {}: GPU time {:+.1} %",
                cases[from].0,
                cases[to].0,
                (b / a - 1.0) * 100.0
            );
        }
        assert!(
            median(&times[0], 0) > 0.0,
            "the card answered no timestamps"
        );
    }
}
