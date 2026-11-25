# Radeon runtime testing, 25 November 2026

## Experimental RT in Chapter 1 City

On an RX 7900 XTX, the runtime reached the outdoor City scene from a copied
autosave with the dynamic structure builder, traversal and lighting resolve
enabled. After GPU completion, a counter on the selected traversal shader
recorded 6,074,512 active node visits and 848 finite, in-range triangle
candidates by frame 1,204. These are cumulative counts for one shader.
They establish that rays intersect scene geometry. Verification of lighting
on individual objects and of every bounce is still open.

The CPU previously declared every mapped guest memory window writable even
when a traversal shader only read through the page directory. The corrected
translator distinguishes guest-memory writes from output to a separate UAV.
Two runs used the same City autosave on the same card, with all three RT
stages and the intersection counter enabled. The sample covers frames 31–80
after the first heavy scene frame in each run.

| City measurement | Before write tracking fix | With write tracking fix |
| --- | ---: | ---: |
| Mean frame time | 3,469 ms | 969 ms |
| Average FPS (50 frames / elapsed time) | 0.29 | 1.03 |
| Median frame time | 3,355 ms | 936 ms |
| 95th-percentile frame time | 4,457 ms | 1,214 ms |
| Guest memory outside registered buffers compared per frame | 12,793 MB | 2,655 MB |
| Wait per frame for guest memory reads | 2,036 ms | 95 ms |
| Draws per frame | 12,583 | 12,520 |

The updated run passed five million cumulative in-range triangle candidates
by frame 1,265. This is one before-and-after measurement on one system.
City can still exhaust the page directory's 1,023 backing-copy slots. A
missing mapping can return zero or lose a write.

## Previous Radeon test results

The Windows diagnostics release at `e061c69032b84ec6e2ec17340469cd689500dafd` built with the pinned toolchain. It includes GPU readback changes from `6a4e2b1` and descriptor allocation changes from `e061c69`.

| Check | Result |
| --- | --- |
| GPU library tests on the D3D12 WARP software adapter | 163 passed, 0 failed, 3 ignored |
| Pixel shaders previously refused in the mission capture | All 11 translated, passed DXIL validation and produced test pipelines |
| Upstream repository asset-policy scan | 182 tracked files passed |

In a preliminary Chapter 1 City comparison, the old and new readback paths used the same checkpoint, shader cache and renderer settings. Across 93 matched indoor presents, frame rate was 3.52 and 4.05 FPS respectively. The 95th-percentile frame time was 432 and 352 ms. Two comparisons of the outdoor camera gave different results, ranging from an 18% loss to an 8% gain. The indoor figure currently has one run of the new build.

The City interior can render with an overexposed doorway and a silhouetted character. The ray-tracing structure builder is disabled by default. Whole-game shader coverage and correct indoor lighting remain open.

## Subsequent shader and pipeline checks

- Before the static sampler fix, the archive scan translated 25,702 shaders and refused 56. Offline tests of those 56 recovered 37 pixel shaders and three mesh shaders. All 40 passed Microsoft's DXIL validator. A synthetic shader run on WARP returned the expected texels for wrap and clamp across four alternating selections.
- A full archive run with the sampler change translated 25,742 of 25,758 shaders. A follow-up test recovered one more pixel shader used for voxel visibility. The corrected translator recognized both halves of a restored wave64 lane mask. Retesting all 56 original refusals produced 41 validated shaders and 15 remaining refusals. The visual effect in the City interior still needs a gameplay comparison.
- On an RX 7900 XTX the original windshield-wiper pipeline failed creation with blending enabled on an integer target. The corrected pipeline created successfully with the same shaders and framebuffer formats. The other active targets retained their blending state and all three write masks were preserved.
- A clean Windows installation built all three programs, resolved all static imports of the tested game executable, and executed a synthetic title through the Start-menu shortcut. Re-running setup preserved existing state.

## City streaming and CPU submission

- Of 2,191 distinct vertex shaders in the tested archive, 39 originally emitted bytecode that failed the Microsoft DXIL validator. Correcting signature order and respecting the declared interpolation validated all 2,191. Eighteen of the corrected shaders rasterize water or its proxy geometry. The 92 shaders selected by temporal and anti-aliasing names also passed validation.
- In the City capture, the native temporal resolve dispatched 320 by 180 groups of 8 by 8 threads. The output was 2560 by 1440. Both bloom and the final composite consumed that output. Temporal blend weights and visual quality across the whole game remain unverified.
- One fixed-camera waterfront capture showed dark materials while thousands of texture slots waited for upload. The queue regression test verifies that new textures get service alongside retries even when the per-pass limit is one. An interior lighting defect persisted after the queue drained.
- In a CPU-only release benchmark of 28,279 small upload reads batched 512 at a time, four workers took 11.67 ms compared with 17.64 ms for one. A separate WARP regression submitted eight interdependent command lists and verified all 128 intermediate texture readbacks with and without parallel prelude recording. These tests measure CPU work and command ordering. Gameplay FPS depends on the scene.

## City save reload and dark objects

During a City save reload, the waterfront house, dock and trees gained detail over 18 seconds. The number of texture views due for upload fell from 10,577 to 5,916 to 36. At the middle sample, shader pipeline creation had already caught up and uploads still averaged 391 MB per frame. The translation cache was cold after a translator change. These measurements include diagnostic captures.

Six seconds before a later patio screenshot, the tabletop item and upright weight plate were already dark in both native TAA outputs. Both objects had similar colors in the final image and the screenshot. That capture did not record the incoming scene color. The same interval averaged 317 skipped ray-tracing structure-builder dispatches per frame, with zero pipelines being built. The game's ray-traced lighting remains incomplete, and the cause of these two objects' color is unresolved.

On an RX 7900 XTX, a standalone DXR 1.1 reflection test hit the first triangle at 2 units, the second at 3 units, and the first again at 3 units. The escaping ray and an outside control ray both missed. All 52 compute shaders from the game's ray-tracing builder also validated and created pipelines on this card. Gameplay needs additional verification of the geometry those shaders build, the rays that traverse it and the resulting lighting.

Five previously refused shadow compute shaders now pass Microsoft's DXIL validator. The runtime created all five compute pipelines on the RX 7900 XTX. Their lighting output still needs comparison in the affected scene.

A second test ran a translated Xbox 64-bit ray-node instruction on the RX 7900 XTX and WARP. Both returned hits at 2, 3 and 3 units across two mirrored triangles, followed by a miss. A closer triangle outside the linked tree was ignored, and a deliberately unmapped guest page produced the expected fault. The GPU read nodes through the runtime's guest-page directory. The test driver used the returned distance and barycentric coordinates to construct each reflected ray between dispatches.

The shader cache now fingerprints 30 translation source files. A controlled change to the host test harness or the standalone intersection tests preserved that fingerprint. Changing either the lowering code or the intersection translator changed it. The revised key requires a fresh translation on first use.

One City texture staging sample spent 846 ms waiting on the global memory mapping lock. The updated staging path takes a lock over the source address range. In a test with a one-second remap, staging an unrelated texture took 77 microseconds. A texture inside the affected range waited for the remap and retained the expected texels.
