# Getting 1.0.2.1 past the loading screen on an RX 7700 XT (running notes)

Kept up to date during the investigation. Newest findings at the end of each section.

## Machine and files

- Windows 11, AMD Radeon RX 7700 XT (RDNA3, 12 GB), Ryzen 5 7600X, 31 GB RAM.
  Two monitors: DISPLAY1 primary 2560x1440, DISPLAY2 1920x1080 (left).
- Game folder in use: `C:\Coding\GTA6 Game files\Grand Theft Auto VI (fixed)` (hard links).
  Package 1.0.2.1; exe must be the untouched one, BLAKE3 `b99f876b74d4a77f…`
  (the emulator then prints `build Series 1.0.2.1`).
- The first USB copy was mangled: every DLL had an `XBE1` header and every `.rpf` an
  `XRP1` header (89 bytes) in front, with the file's original length kept, so the end
  of each file was lost. The user re-copied clean files (`fixed dll`, `fixedrpf`) and
  20 files that had not been copied at all (shaders/packs/textures/script rpf,
  story000_* packs, vcruntime/XboxBvhBuilder/XCurl dlls, …).
  `pack\story000\story000.rpf` is still in the wrapped form (header + complete archive).
- `appdata0.rpf` is genuinely 8,880 bytes (a fresh copy from the Xbox matched byte for
  byte). The game reads past its end at boot (8,192 from 4,096, then 3,408 at 8,880);
  that is read-ahead, not a truncated file. Ruled out.

## Emulator changes made on branch `wrapped-package-files`

- `XBE1`/`XRP1` wrapped files are read through (loader skips the DLL header; file
  layer shifts archive offsets, reports full size, zero-fills the lost end).
- `VICTORY_MONITOR=<n>` puts the window on `\\.\DISPLAY<n>`.
- Host pads read at rest while another program has focus; `VICTORY_NO_PADS=1`
  ignores host pads (keyboard stand-in pad only).
- Diagnostics: async calls not finished after 5 s; failed/empty title reads; first
  60 reads; XUser calls; game-save calls; `VICTORY_STACK_REGS=<+rva>` prints
  callee-saved registers (and what they point to) at that frame in `VICTORY_STACKS`
  dumps.
- Merged Belobug `main` (10 commits).

## How to drive a run without a person

- Diag build: `cargo build --profile diag -p victory --features diagnostics`.
- `VICTORY_KEYS_FILE=<file>`: each line appended later is pressed once (0.2 s).
  Names: `Enter Esc Tab Back Up Down Left Right` or a letter/digit
  (`Escape`/`Backspace` are NOT valid and are silently ignored).
- `VICTORY_CAPTURE=<dir>` + `VICTORY_CAPTURE_FRAMES=60,120,…` to see the screen.
- First-boot sequence: Enter through narration prompt, intro art, brightness,
  "FINISH SETUP?" (A). On LEGAL NOTICES: `Down`, wait ~7 s, `Down`, `Enter`.
  Timing varies run to run (Legal Notices at 40–75 s), so drive it live from captures.
- `VICTORY_FPS=30` matches the console pace (fork default caps at 240).

## What happens

- Hardware D3D12 path: boot → intro → narration prompt → brightness → finish setup →
  Legal Notices (lock on Copyright, Next greyed with countdown) → dark screen with a
  small spinner forever (5 draws/frame, 0 disk reads, 1 core busy).
  The account-picker / start screen never shows its prompts on hardware.
- WARP (`VICTORY_D3D12_WARP=1`, ~25 fps on the CPU): the start screen shows
  "ACCOUNT PICKER (Y) / SCREEN NARRATION (X) / PLAYER (A)"; Legal Notices loads fully;
  after Next it ALSO sits on the dark spinner (same as Marvisak/VIctory#1063 reports).
- Same result on: this fork, Marvisak main (`63bcca4`), Marvisak `ffe310e`,
  codec-d/cool_fox (run by the user). Not a regression.

## Where it is stuck (from stacks + disassembly of 1.0.2.1)

- Main thread: `+0x1621ae0` (.zmt resource loader) → stream read `+0x51d2260` →
  device read `+0x13308e0` (inflate loop) → `+0x1330cd0` (chunk loop, picks a part
  device by index) → device vtable +0x30 → `+0x1331210` → …
- Stream object at the stuck frame: position 0, 0 bytes buffered, 0x1000 buffer: its
  first read never returns. Registers at `+0x51d2319`: r15 =
  `platform:/levels/americas/navmeshes/navmeshes_swaps.zmt` (r14 = length 55).
- Device vtable `+0x62fd480` = the title's codec provider (#1063). No RTTI in the exe.
- `+0x13308e0` loops while compressed bytes remain; if the raw read
  (`+0x1330b60`) returns 0, it never ends. #1063: `FUN_141330cd0`'s guard fails and
  `[inst+0x20]` only drops on success; the producer is title code (reader vtable
  `+0x62fc550`, `+0xcdd930` → `+0x313fc0` sync ReadFile reader / global block reader)
  and leaves the stream buffer empty. During the spin no ReadFile/DStorage call is made.
- Not the cause (measured): emulator version, frame pacing, network (blocked, only a
  hostname lookup), saves (provider only, no containers), XUser (all calls in first
  10 s), XAsync (nothing pending), package chunks (all "installed"), Windows locale
  (en-US), monitor/pads.

## ROOT CAUSE (2026-11-26, ~03:45): navmeshes.rpf is cut short

- Following the loop down: `+0x1330b60` (raw compressed read) → plain path
  `+0x1330c10` → `reader->vt[0x60]` = `+0xcdd930`, which returns 0 when the file
  object is -1 or **when the offset is at or past the file object's size**.
  The inflate loop in `+0x13308e0` then never ends (input stays empty, no error).
- `VICTORY_STACK_REGS=+0x133096e` with `VICTORY_STACK_PEEK=rbx+8>0,rbx+8>0x18,…`
  (rbx = the codec provider, [rbx+8] = the archive's storage object) shows:
  storage `+0x28` = archive size `0x45ce0000`; part table `+0x50` = null, so the
  default file `+0x18` is used; that file object's size `+8` = `0x45ce0000`.
  That is exactly the size of our `navmeshes.rpf` (1,171,128,320 bytes).
- So the table of contents of navmeshes.rpf places `navmeshes_swaps.zmt` (204,848
  compressed bytes) past the end of the file we have: **the copy is truncated**.
  `0x45ce0000` is an exact multiple of 64 KiB; no complete archive here is.
- Only one other file has a 64 KiB-multiple size: `data\ttdb\peds\decorations_1_3.ttpf`
  (`0x1e660000`) — suspicious, recopy it too.
- Fix: copy `navmeshes.rpf` (and `decorations_1_3.ttpf`) from the Xbox again and check
  the new size is larger and not a multiple of 64 KiB. No emulator change can supply
  the missing data.
- Also done: `pack\story000\story000.rpf` in the (fixed) folder is now the plain
  archive (wrapped header stripped); the wrapped original stays in the original folder.

## After a full navmeshes.rpf (2026-11-26, afternoon)

- A fresh `navmeshes.rpf` (6,707,512,496 bytes, not a 64 KiB multiple) gets past the
  hang: the game shows ROCKSTAR GAMES LEGAL TERMS & POLICIES next. Tick both boxes
  (Privacy Policy and Terms of Service, Enter on each) and Confirm. It then opens the
  story packs (story000*, shared_minigame000*) and reaches `Game State: Game Running`.
- About 5 s later it stops itself (`0x80000003` at `+0x29defb1`, a status check).
  DirectStorage failure logs now name the file: a ZLIB request at offset
  20,022,473,968 in `pack\story000\story000.rpf`, which is only 16,577,789,568 bytes
  (`0x80070026`, end of file). So **story000.rpf is cut short as well**; the
  "length minus header is 16-byte aligned" test that called it complete was wrong.
- Lesson: file sizes alone cannot prove a copy is complete. The only reliable check is
  the size the Xbox reports for each file, or the game reading every entry.

## Leads

- Marvisak/VIctory#1063: same loader; on GTX 1660 it stops at `+0x1330a44` (codec
  0x817, `platform:/levels/americas/map/powergrids.zmt`); WARP passes Legal Notices;
  ZSayerHQ/victory-world-runtime is said to reach the world on RDNA3/RTX30+/5090.
- Fork survey in progress (fixes for the loader hang, AMD RDNA3 world runs, speed).

## A full copy, then in-world (2026-11-26, evening)

- The user copied the whole game folder again, to `D:\GTA 6 game files\GTA 6\GTA6`
  (kept as the backup) and over `Grand Theft Auto VI (fixed)` on C:. Only
  `story000.rpf` differed in size (now 20,022,508,672 bytes); no file is wrapped any
  more. The folder from the Xbox has no `game_scarlett_master.exe`: copy the untouched
  one beside it (BLAKE3 `b99f876b…`).
- With it the title loaded the story and ran ~3 minutes at 0.3 fps (3.5 s frames,
  copies ~0.7 s), then ended itself: `0xc0000005` writing 0 at `+0x23cf683`. That is a
  deliberate fatal routine (`+0x23cf620`): it decodes a 12-byte string
  (`out[i] = in[i] + (12 - i)` over `3B 45 4B 17 44 68 5D 1B 4F 66 65 FF`) to
  **"GPU Loc Sig"** and writes `0x4D8BD421` to address 0: the title's GPU-lockup
  watchdog. Same as Belobug/VIctory-Belobug-AMD#344.

## Branch `rx9070xt-speed` (Belobug `rx9070xt` + ours)

- Belobug's `rx9070xt` branch was rebased away from `main`/Marvisak (589 commits we
  lacked, 57 conflicts on merge), so `rx9070xt-speed` starts from it and re-applies only
  our monitor, pads-at-rest, stack-dump and notes commits; the wrapped-file support was
  dropped (the files are clean now). `VICTORY_KEYS_FILE` was ported (it lives on
  Belobug `main` only).
- Shaders: `victory shaders <game dir>` translated 25,790 shaders in ~3.5 minutes (195
  "containers without a program" are not shaders); before, the cache held 1,438.
  `VICTORY_PORT_CBV=1` is part of the cache key: translate with the switch you play with.
- Pipelines: `victory warm` needs `known-pipelines.bin`, written only by runs with
  `VICTORY_PIPELINE_BOOK=1`; one ~15 minute session recorded 83,594.
- Run: play.ps1's switches + `VICTORY_PORT_CONSTANTS/DESCRIPTORS/CBV/GUARD/JOURNAL=1`,
  `VICTORY_RESIDENCY=1`, `VICTORY_LEVELS_ON_DEMAND=1`, `--console series-s`.
  Result: boots to the start screen, the opening cutscenes and gameplay. Loading
  ~3 fps (copies 10 ms instead of 700 ms); in the world 13–21 fps, dropping to ~3 fps
  while new pipelines are made (their draws are dropped meanwhile, so the picture fills
  in over time). Memory commit ~22 GB. No shader failed to translate.
- Picture still wrong: dark brown haze, horizontal streaks, speckled surfaces.
- Where a world frame goes (copystat, ~47–75 ms): watches ~15 ms checking ~4 GB of which
  ~3.96 GB is unchanged; guard-lock waits ~45 ms summed over threads; ~10,000 page
  faults; copies 12–24 ms; ~15 ms waiting for pipelines.
- The title's memory flags (`note_protection`): CPU+GPU read-write `0x2004` ~4 GB;
  write-combined CPU+GPU `0x2404` ~3 GB; coherent `0x22004` ~1.4 GB; CPU-only `0x4`
  ~16.6 GB. **Nothing is GPU-read-only**, so "upload once, never check" by flag does not
  apply; write-combined memory is the candidate for being read in place.
- A later boot (terms accepted) goes straight to the start screen; press A (Enter).
  "title suspended; ending the title" = the window was closed/minimised, not a crash.

## Leads (evening)

- Subagents on branches: `picture-fixes` (Marvisak commits missing from rx9070xt),
  `fix-344-gpu-watchdog`, `fix-350-337`, `fix-shader-352-3366`, `fix-355-skinning`;
  PS4/PS5 pads merged (`playstation.rs`); a fork survey.
- Memory hints: route write-combined (`0x400`) title memory to `inplace::place_for`;
  per-flag-class tallies of checked/copied bytes in copystat first.
